use std::{
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process,
    time::SystemTime,
};

use serde_json::Value;
use sha1::{Digest, Sha1};

use crate::nix_command;

#[derive(Debug, Clone)]
pub struct NixProfileCache {
    cache_dir: PathBuf,
    flake_inputs_dir: PathBuf,
    flake_reference: FlakeReference,
    files_to_watch: Vec<PathBuf>,
    profile_symlink: PathBuf,
    profile_rc_file: PathBuf,
}

impl NixProfileCache {
    pub fn new(cache_dir: PathBuf, flake_reference: &str) -> anyhow::Result<Self> {
        let flake_inputs_dir = cache_dir.join("flake-inputs");

        let flake_reference = FlakeReference::parse(flake_reference)?;

        let mut files_to_watch = vec![];
        let hash = if let Some(flake_dir) = &flake_reference.flake_dir {
            files_to_watch.extend_from_slice(&[
                flake_dir.join("flake.nix"),
                flake_dir.join("flake.lock"),
                flake_dir.join("devshell.toml"),
            ]);
            hash_files(&files_to_watch)?
        } else {
            hash_flake_reference(&flake_reference.flake_reference_string)?
        };

        let profile_symlink = cache_dir.join(format!("flake-profile-{}", hash));
        let profile_rc_file = profile_symlink.with_extension("rc");
        Ok(Self {
            cache_dir,
            flake_inputs_dir,
            flake_reference,
            files_to_watch,
            profile_symlink,
            profile_rc_file,
        })
    }

    pub fn needs_update(&self) -> anyhow::Result<bool> {
        let mut need_update = true;

        if self.profile_rc_file.is_file() && self.profile_symlink.is_file() {
            let profile_rc_mtime = fs::metadata(&self.profile_rc_file)?.modified()?;

            need_update = self.files_to_watch.iter().any(|file| {
                fs::metadata(file)
                    .and_then(|meta| meta.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH)
                    > profile_rc_mtime
            });
        }

        Ok(need_update)
    }

    pub fn update(&self) -> anyhow::Result<()> {
        clean_old_gcroots(&self.cache_dir, &self.flake_inputs_dir)?;

        let tmp_profile = self
            .cache_dir
            .join(format!("flake-tmp-profile.{}", process::id()));

        let stdout_content = nix_command::nix([
            OsStr::new("print-dev-env"),
            OsStr::new("--profile"),
            tmp_profile.as_os_str(),
            OsStr::new(&self.flake_reference.flake_reference_string),
        ])?;

        fs::File::create(&self.profile_rc_file)?.write_all(stdout_content.as_bytes())?;

        add_gcroot(&tmp_profile, &self.profile_symlink)?;
        fs::remove_file(&tmp_profile)?;

        if self.flake_reference.flake_dir.is_some() {
            for input in get_flake_input_paths(&self.flake_reference.flake_reference_string)? {
                let store_path = PathBuf::from("/nix/store").join(&input);
                let symlink_path = self.flake_inputs_dir.join(&input);
                add_gcroot(&store_path, &symlink_path)?;
            }
        }

        Ok(())
    }

    pub fn profile_rc(&self) -> &Path {
        &self.profile_rc_file
    }
}

#[derive(Debug, Clone)]
struct FlakeReference {
    pub flake_reference_string: String,
    pub flake_dir: Option<PathBuf>,
}

impl FlakeReference {
    pub fn parse(flake_reference: &str) -> anyhow::Result<Self> {
        let mut flake_reference_iter = flake_reference.split('#');
        let flake_uri = flake_reference_iter
            .next()
            .ok_or_else(|| anyhow::anyhow!("Missing flake URI"))?;
        let flake_specifier = flake_reference_iter.next();

        let expanded_flake_reference_and_flake_dir =
            if FlakeReference::is_path_type(flake_reference) {
                let flake_dir_str =
                    shellexpand::full(flake_uri.strip_prefix("path:").unwrap_or(flake_uri))?;
                let expanded_flake_reference = format!(
                    "{}{}{}",
                    &flake_dir_str,
                    flake_specifier.map(|_| "#").unwrap_or(""),
                    flake_specifier.unwrap_or("")
                );
                Some((expanded_flake_reference, flake_dir_str))
            } else {
                None
            };

        Ok(Self {
            flake_dir: expanded_flake_reference_and_flake_dir
                .as_ref()
                .map(|x| PathBuf::from(x.1.to_string())),
            flake_reference_string: expanded_flake_reference_and_flake_dir
                .map(|x| x.0)
                .unwrap_or_else(|| String::from(flake_reference)),
        })
    }

    fn is_path_type(flake_reference: &str) -> bool {
        flake_reference.starts_with("path:")
            || flake_reference.starts_with('~')
            || flake_reference.starts_with('/')
            || flake_reference.starts_with("./")
            || flake_reference.starts_with("../")
    }
}

fn hash_files(filenames: impl AsRef<[PathBuf]>) -> anyhow::Result<String> {
    let (hasher, no_files) = filenames
        .as_ref()
        .iter()
        .filter(|f| {
            // TODO: figure out what to do if the file doesn't exist
            f.exists()
        })
        .try_fold((Sha1::new(), true), |(mut acc, ..), f| {
            acc.update(fs::read(f)?);
            anyhow::Result::<(Sha1, bool)>::Ok((acc, false))
        })?;

    if no_files {
        return Err(anyhow::anyhow!("No files found to hash"));
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_flake_reference(flake_reference: &str) -> anyhow::Result<String> {
    let mut hasher = Sha1::new();
    hasher.update(flake_reference);
    Ok(format!("{:x}", hasher.finalize()))
}

fn clean_old_gcroots(cache_dir: &Path, flake_inputs_dir: &Path) -> anyhow::Result<()> {
    let res = fs::remove_dir_all(cache_dir);
    if let Err(e) = &res
        && e.kind() != io::ErrorKind::NotFound
    {
        res?;
    }
    fs::create_dir_all(flake_inputs_dir)?;
    Ok(())
}

fn add_gcroot(store_path: &Path, symlink: &Path) -> anyhow::Result<()> {
    nix_command::nix([
        OsStr::new("build"),
        OsStr::new("--out-link"),
        symlink.as_os_str(),
        store_path.as_os_str(),
    ])?;
    Ok(())
}

fn get_flake_input_paths(flake_reference: &str) -> anyhow::Result<Vec<PathBuf>> {
    let stdout_content = nix_command::nix([
        "flake",
        "archive",
        "--json",
        "--no-write-lock-file",
        flake_reference,
    ])?;
    let json = serde_json::from_str::<Value>(&stdout_content)?;
    Ok(get_paths_from_doc(&json))
}

fn get_paths_from_doc(doc: &Value) -> Vec<PathBuf> {
    let mut result = Vec::new();

    if let Some(p) = get_path(doc) {
        result.push(p);
    }

    if let Some(inputs) = doc.get("inputs").and_then(|i| i.as_object()) {
        for (_k, v) in inputs {
            let sub_paths = get_paths_from_doc(v);
            result.extend(sub_paths);
        }
    }

    result
}

fn get_path(doc: &Value) -> Option<PathBuf> {
    doc.get("path")
        .and_then(|value| value.as_str())
        .map(|path| {
            if path.len() > 11 {
                PathBuf::from(&path[11..])
            } else {
                PathBuf::from(path)
            }
        })
}

#[cfg(test)]
mod tests {
    use std::{io::Write, path::PathBuf};

    use once_cell::sync::Lazy;
    use serde_json::json;
    use tempfile::NamedTempFile;

    use super::{get_path, get_paths_from_doc, hash_files};

    static TEST_FILE: Lazy<NamedTempFile> = Lazy::new(|| {
        let mut test_file = tempfile::NamedTempFile::new().unwrap();
        writeln!(test_file.as_file_mut(), r#"echo "1.1.1";"#).unwrap();
        test_file
    });

    #[test]
    fn test_hash_one() {
        assert_eq!(
            hash_files([TEST_FILE.path().to_path_buf()]).unwrap(),
            "6ead949bf4bcae230b9ed9cd11e578e34ce9f9ea"
        );
    }

    #[test]
    fn test_hash_multiple() {
        assert_eq!(
            hash_files([
                TEST_FILE.path().to_path_buf(),
                TEST_FILE.path().to_path_buf(),
            ])
            .unwrap(),
            "f109b7892a541ed1e3cf39314cd25d21042b984f"
        );
    }

    #[test]
    fn test_hash_filters_nonexistent() {
        assert_eq!(
            hash_files([TEST_FILE.path().to_path_buf(), PathBuf::from("FOOBARBAZ"),]).unwrap(),
            "6ead949bf4bcae230b9ed9cd11e578e34ce9f9ea"
        );
    }

    #[test]
    fn test_get_path_removes_prefix() {
        let input = json!({
            "path": "aaaaaaaaaaabbbbb"
        });
        let result = get_path(&input);
        assert_eq!(result, Some(PathBuf::from("bbbbb")));
    }

    #[test]
    fn test_get_paths_from_doc() {
        let input = json!({
            "path": "aaaaaaaaaaabbbbb",
            "inputs": {
                "foo": {
                    "path": "aaaaaaaaaaaccccc",
                    "inputs": {
                        "bar": {
                            "path": "aaaaaaaaaaaddddd",
                            "inputs": {}
                        }
                    }
                }
            }
        });
        let result = get_paths_from_doc(&input);
        assert_eq!(
            result,
            vec![
                "bbbbb".to_string(),
                "ccccc".to_string(),
                "ddddd".to_string()
            ]
        );
    }
}
