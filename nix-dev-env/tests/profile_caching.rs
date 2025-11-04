use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{self, Command},
};

use nix_dev_env::NixProfileCache;
use tempfile::{tempdir, tempdir_in};

#[test]
fn test_nix_profile_cache() {
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
    let tmp_profile = cache_dir
        .path()
        .join(format!("flake-tmp-profile.{}", process::id()));
    let profile_symlink = cache_dir
        .path()
        .join("flake-profile-bf21a9e8fbc5a3846fb05b4fa0859e0917b2202f");
    let mut profile_rc = profile_symlink.clone();
    profile_rc.set_extension("rc");
    let flake_inputs_path = cache_dir.path().join("flake-inputs");

    let profile_rc_content = "export FAKE_VAR=true;";
    let nixpkgs_dir_name = "yfzmnk75f009yb7b542kf4r7qaqq9kid-source";
    let nixpkgs_path = format!("/nix/store/{nixpkgs_dir_name}");
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
    echo '{{ "inputs": {{ "nixpkgs": {{ "inputs": {{}}, "path": "{nixpkgs_path}" }} }} }}'
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

    let nix_profile_cache = NixProfileCache::new(
        PathBuf::from(cache_dir.path()),
        &format!("path:{}", flake_dir.path().to_string_lossy()),
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

    let mut cache_entries = fs::read_dir(cache_dir.path())
        .unwrap()
        .map(|entry| entry.map(|e| e.path()))
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    cache_entries.sort();
    assert_eq!(
        cache_entries,
        [
            flake_inputs_path.clone(),
            profile_symlink.clone(),
            profile_rc.clone(),
        ]
    );
    assert_eq!(
        fs::read_to_string(&profile_rc).unwrap(),
        format!("{profile_rc_content}\n")
    );

    let log_contents = fs::read_to_string(&log_file).unwrap();
    let log_lines = log_contents
        .split('\n')
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    assert_eq!(
        log_lines,
        [
            format!(
                "--extra-experimental-features nix-command flakes print-dev-env --impure --no-write-lock-file --profile {tmp_profile} {flake_dir}",
                tmp_profile = tmp_profile.to_string_lossy(),
                flake_dir = flake_dir.path().to_string_lossy()
            ),
            format!(
                "--extra-experimental-features nix-command flakes build --impure --out-link {profile_symlink} {tmp_profile}",
                profile_symlink = profile_symlink.to_string_lossy(),
                tmp_profile = tmp_profile.to_string_lossy()
            ),
            format!(
                "--extra-experimental-features nix-command flakes flake archive --impure --json --no-write-lock-file {flake_dir}",
                flake_dir = flake_dir.path().to_string_lossy()
            ),
            format!(
                "--extra-experimental-features nix-command flakes build --impure --out-link {flake_inputs_symlink} {nixpkgs_path}",
                flake_inputs_symlink = flake_inputs_path.join(nixpkgs_dir_name).to_string_lossy()
            ),
        ]
    );

    unsafe {
        env::set_var("PATH", original_path);
    }
}
