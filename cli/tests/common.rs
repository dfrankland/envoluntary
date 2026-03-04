use std::{env, fs, os::unix::fs::PermissionsExt, path::PathBuf};

use tempfile::TempDir;

pub struct Fixtures {
    pub work_dir: TempDir,
    pub cache_dir: TempDir,
    pub config_file: PathBuf,
    pub bin_dir: PathBuf,
    pub path: String,
}

pub fn build_fixtures() -> Fixtures {
    let work_dir = tempfile::tempdir().unwrap();
    let cache_dir = tempfile::tempdir_in(work_dir.path()).unwrap();

    let config_file = setup_mock_config(work_dir.path());
    let bin_dir = setup_mock_nix_bin(work_dir.path());

    let original_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), original_path);
    Fixtures {
        work_dir,
        cache_dir,
        config_file,
        bin_dir,
        path: new_path,
    }
}

pub fn setup_mock_config(work_dir: &std::path::Path) -> std::path::PathBuf {
    let config_file = work_dir.join("config.toml");
    fs::write(
        &config_file,
        toml::to_string_pretty(&toml::toml! {
            [[entries]]
            pattern = "^/some/dir(/.*)?"
            flake_reference = "github:owner/repo"

            [[entries]]
            pattern = "~/some/other/dir(/.*)?"
            flake_reference = "github:other_github_owner/repo"

            [[entries]]
            pattern = ".*"
            flake_reference = "github:owner/super_cool_tool"
            pattern_adjacent = ".*/\\.supercooltool"

            [[entries]]
            pattern = ".*"
            flake_reference = "github:owner/awesome_tool"
            pattern_adjacent = "~/\\.awesometool"
        })
        .unwrap(),
    )
    .unwrap();
    config_file
}

pub fn setup_mock_nix_bin(work_dir: &std::path::Path) -> std::path::PathBuf {
    let bin_dir = work_dir.join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let nix_file = bin_dir.join("nix");

    let bash_path = env::var("NIX_BIN_BASH").unwrap_or_else(|_| String::from("/bin/bash"));
    let profile_rc_content = "export FAKE_VAR=true;";

    let nix_file_content = format!(
        r#"#! {bash_path}

if [[ "$@" == "--extra-experimental-features nix-command flakes --version" ]]; then
    echo "nix (Nix) 2.30.0"
elif [[ "$@" == "--extra-experimental-features nix-command flakes print-dev-env --no-write-lock-file --profile "* ]]; then
rc="{profile_rc_content}"
for ((i=0; i<$#; i++)); do
    if [[ "${{@:$i:1}}" == "--profile" ]]; then
        profile_path="${{@:$((i+1)):1}}"
        echo "$rc" > "$profile_path"
        break
    fi
done
echo "$rc"
elif [[ "$@" == "--extra-experimental-features nix-command flakes build --out-link "* ]]; then
for ((i=0; i<$#; i++)); do
    if [[ "${{@:$i:1}}" == "--out-link" ]]; then
        link_path="${{@:$((i+1)):1}}"
        installable="${{@:$((i+2)):1}}"
        mkdir -p "$(dirname "$link_path")"
        ln -sf "$installable" "$link_path"
        break
    fi
done
elif [[ "$@" == "--extra-experimental-features nix-command flakes flake archive --json --no-write-lock-file "* ]]; then
echo '{{ "inputs": {{ "nixpkgs": {{ "inputs": {{}}, "path": "/nix/store/yfzmnk75f009yb7b542kf4r7qaqq9kid-source" }} }} }}'
fi

exit 0
"#
    );

    fs::write(&nix_file, nix_file_content).unwrap();
    fs::set_permissions(&nix_file, fs::Permissions::from_mode(0o755)).unwrap();
    bin_dir
}
