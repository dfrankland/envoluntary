use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{self, Command},
};

use nix_dev_env::NixProfileCache;
use tempfile::{TempDir, tempdir, tempdir_in};

struct TestEnv {
    _work_dir: TempDir,
    cache_dir: TempDir,
    flake_dir: TempDir,
    log_file: PathBuf,
    original_path: String,
}

impl TestEnv {
    fn new() -> Self {
        let work_dir = tempdir().unwrap();
        let cache_dir = tempdir_in(work_dir.path()).unwrap();
        let bin_dir = work_dir.path().join("bin");
        fs::create_dir(&bin_dir).unwrap();
        let nix_file = bin_dir.join("nix");
        let log_file = work_dir.path().join("nix_commands.log");
        let bash_path = env::var("NIX_BIN_BASH").unwrap_or_else(|_| String::from("/bin/bash"));
        let flake_dir = tempdir_in(work_dir.path()).unwrap();
        let flake_file = flake_dir.path().join("flake.nix");
        fs::write(flake_file, "{}").unwrap();

        let profile_rc_content = "export FAKE_VAR=true;";
        let nix_file_content = format!(
            r#"#! {bash_path}

echo "$@" >> "{log_file}"

if [[ "$@" == "--extra-experimental-features nix-command flakes print-dev-env --impure --no-write-lock-file --profile "* ]]; then
    rc="{profile_rc_content}"
    for ((i=0; i<$#; i++)); do
        if [[ "${{@:$i:1}}" == "--profile" ]]; then
            profile_path="${{@:$((i+1)):1}}"
            echo "$rc" > "$profile_path"
            break
        fi
    done
    echo "$rc"
elif [[ "$@" == "--extra-experimental-features nix-command flakes build --impure --out-link "* ]]; then
    for ((i=0; i<$#; i++)); do
        if [[ "${{@:$i:1}}" == "--out-link" ]]; then
            link_path="${{@:$((i+1)):1}}"
            installable="${{@:$((i+2)):1}}"
            mkdir -p "$(dirname "$link_path")"
            ln -sf "$installable" "$link_path"
            break
        fi
    done
elif [[ "$@" == "--extra-experimental-features nix-command flakes flake archive --impure --json --no-write-lock-file "* ]]; then
    echo '{{ "inputs": {{ "nixpkgs": {{ "inputs": {{}}, "path": "{NIXPKGS_PATH}" }} }} }}'
fi

exit 0
"#,
            log_file = log_file.display()
        );
        fs::write(&nix_file, nix_file_content).unwrap();
        fs::set_permissions(&nix_file, fs::Permissions::from_mode(0o755)).unwrap();

        let original_path = env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{}", bin_dir.display(), original_path);
        unsafe {
            env::set_var("PATH", &new_path);
        }

        Self {
            _work_dir: work_dir,
            cache_dir,
            flake_dir,
            log_file,
            original_path,
        }
    }

    fn flake_dir_path(&self) -> &Path {
        self.flake_dir.path()
    }

    fn cache_dir_path(&self) -> &Path {
        self.cache_dir.path()
    }

    fn read_log_lines(&self) -> Vec<String> {
        fs::read_to_string(&self.log_file)
            .unwrap()
            .split('\n')
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect()
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        unsafe {
            env::set_var("PATH", &self.original_path);
        }
    }
}

const NIXPKGS_DIR_NAME: &str = "yfzmnk75f009yb7b542kf4r7qaqq9kid-source";
const NIXPKGS_PATH: &str = "/nix/store/yfzmnk75f009yb7b542kf4r7qaqq9kid-source";
const PROFILE_HASH: &str = "bf21a9e8fbc5a3846fb05b4fa0859e0917b2202f";

fn run_profile_cache_test(hash_fragment: Option<&str>) {
    let env = TestEnv::new();

    let flake_dir_str = env.flake_dir_path().to_string_lossy();
    let flake_reference = match hash_fragment {
        Some(fragment) => format!("path:{flake_dir_str}#{fragment}"),
        None => format!("path:{flake_dir_str}"),
    };
    let expected_print_dev_env_ref = match hash_fragment {
        Some(fragment) => format!("{flake_dir_str}#{fragment}"),
        None => flake_dir_str.to_string(),
    };

    let tmp_profile = env
        .cache_dir_path()
        .join(format!("flake-tmp-profile.{}", process::id()));
    let profile_symlink = env
        .cache_dir_path()
        .join(format!("flake-profile-{}", PROFILE_HASH));
    let flake_inputs_path = env.cache_dir_path().join("flake-inputs");

    let nix_profile_cache = NixProfileCache::new(
        PathBuf::from(env.cache_dir_path()),
        &flake_reference,
        nix_dev_env::EvaluationMode::Impure,
    )
    .unwrap();

    assert!(nix_profile_cache.needs_update().unwrap());
    nix_profile_cache.update().unwrap();
    assert!(!nix_profile_cache.needs_update().unwrap());

    assert!(
        fs::metadata(nix_profile_cache.profile_rc())
            .unwrap()
            .is_file()
    );

    let exit_status = Command::new("bash")
        .args([nix_profile_cache.profile_rc()])
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
    assert_eq!(exit_status.code().unwrap(), 0);

    let log_lines = env.read_log_lines();
    assert_eq!(
        log_lines,
        [
            format!(
                "--extra-experimental-features nix-command flakes print-dev-env --impure --no-write-lock-file --profile {} {}",
                tmp_profile.to_string_lossy(),
                expected_print_dev_env_ref
            ),
            format!(
                "--extra-experimental-features nix-command flakes build --impure --out-link {} {}",
                profile_symlink.to_string_lossy(),
                tmp_profile.to_string_lossy()
            ),
            format!(
                "--extra-experimental-features nix-command flakes flake archive --impure --json --no-write-lock-file {}",
                flake_dir_str
            ),
            format!(
                "--extra-experimental-features nix-command flakes build --impure --out-link {} {}",
                flake_inputs_path.join(NIXPKGS_DIR_NAME).to_string_lossy(),
                NIXPKGS_PATH
            ),
        ]
    );
}

#[test]
fn test_nix_profile_cache() {
    run_profile_cache_test(None);
}

#[test]
fn test_nix_profile_cache_with_hash_fragment() {
    run_profile_cache_test(Some("myDevShell"));
}
