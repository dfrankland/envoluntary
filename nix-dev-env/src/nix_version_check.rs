use std::ffi::OsStr;

use once_cell::sync::Lazy;
use regex::Regex;
use semver::{Comparator, Op, Prerelease, Version, VersionReq};

use crate::nix_command;

static REQUIRED_NIX_VERSION: Lazy<VersionReq> = Lazy::new(|| VersionReq {
    comparators: vec![Comparator {
        op: Op::GreaterEq,
        major: 2,
        minor: Some(10),
        patch: Some(0),
        pre: Prerelease::EMPTY,
    }],
});

static SEMVER_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"([0-9]+\.[0-9]+\.[0-9]+)").unwrap());

pub fn check_nix_version() -> anyhow::Result<()> {
    check_nix_program_version(OsStr::new("nix"))
}

fn check_nix_program_version(nix_executable_path: impl AsRef<OsStr>) -> anyhow::Result<()> {
    let stdout_content = nix_command::nix_program(nix_executable_path.as_ref(), ["--version"])?;

    if stdout_content.is_empty() {
        return Err(anyhow::format_err!("`nix --version` failed to execute."));
    }

    let nix_version_match = SEMVER_RE
        .find(&stdout_content)
        .ok_or_else(|| anyhow::format_err!("SemVer from `nix --version` could not be found."))?;
    let nix_version = Version::parse(nix_version_match.as_str())?;

    if REQUIRED_NIX_VERSION.matches(&nix_version) {
        Ok(())
    } else {
        Err(anyhow::format_err!("`nix` version too old for flakes."))
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf};

    use super::check_nix_program_version;

    #[derive(Debug)]
    struct NixExecutable {
        // NB: `_dir` needed to prevent tempfile cleanup
        pub _dir: tempfile::TempDir,
        pub file_path: PathBuf,
    }

    impl NixExecutable {
        fn new(file_contents: &str) -> Self {
            // NB: Use a temp dir instead of a temp file since executing a file requires the file is
            // not open for writing / deleting
            let dir = tempfile::tempdir().unwrap();
            let file_path = dir.path().join("nix");
            let bash_path = env::var("NIX_BIN_BASH").unwrap_or_else(|_| String::from("/bin/bash"));
            fs::write(&file_path, format!("#! {bash_path}\n{file_contents}")).unwrap();
            fs::set_permissions(&file_path, fs::Permissions::from_mode(0o777)).unwrap();
            Self {
                _dir: dir,
                file_path,
            }
        }
    }

    #[test]
    fn test_error_on_exit_failure() {
        let nix_executable = NixExecutable::new(r#"exit 1;"#);
        assert_eq!(
            check_nix_program_version(&nix_executable.file_path)
                .unwrap_err()
                .to_string(),
            format!(
                "`{} --extra-experimental-features nix-command' flakes' --version` failed with error:\nprocess exited unsuccessfully: exit status: 1",
                nix_executable.file_path.to_string_lossy()
            )
        );
    }

    #[test]
    fn test_error_on_empty_stdout() {
        let nix_executable = NixExecutable::new(r#"printf "";"#);
        assert_eq!(
            check_nix_program_version(nix_executable.file_path)
                .unwrap_err()
                .to_string(),
            "`nix --version` failed to execute."
        );
    }

    #[test]
    fn test_error_on_missing_semver() {
        let nix_executable = NixExecutable::new(r#"echo "hello";"#);
        assert_eq!(
            check_nix_program_version(nix_executable.file_path)
                .unwrap_err()
                .to_string(),
            "SemVer from `nix --version` could not be found."
        );
    }

    #[test]
    fn test_error_on_too_old_version() {
        let nix_executable = NixExecutable::new(r#"echo "nix (Nix) 0.0.0";"#);
        assert_eq!(
            check_nix_program_version(nix_executable.file_path)
                .unwrap_err()
                .to_string(),
            "`nix` version too old for flakes."
        );
    }

    #[test]
    fn test_version_matches_minimum() {
        let nix_executable = NixExecutable::new(r#"echo "nix (Nix) 2.10.0";"#);
        check_nix_program_version(nix_executable.file_path).unwrap();
    }

    #[test]
    fn test_version_matches_newer() {
        let nix_executable = NixExecutable::new(r#"echo "nix (Nix) 2.30.0";"#);
        check_nix_program_version(nix_executable.file_path).unwrap();
    }
}
