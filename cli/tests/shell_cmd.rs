use std::{env, fs, os::unix::fs::PermissionsExt, process};

use assert_cmd::{Command, cargo};
use env_hooks::{BashSource, EnvVars, get_env_vars_from_bash};
use predicates::prelude::*;
use sha1::{Digest, Sha1};

#[test]
fn shell_hook_bash_produces_evaluable_shell_syntax() {
    let mut cmd = Command::new(cargo::cargo_bin!());
    cmd.args(["shell", "hook", "bash"]);

    let export_output = cmd.output().unwrap();
    assert!(export_output.status.success());

    let bash_script = String::from_utf8_lossy(&export_output.stdout);

    assert!(!bash_script.contains("{{."));

    let bash_export = process::Command::new("bash")
        .arg("-c")
        .arg(bash_script.as_ref())
        .output()
        .unwrap();

    assert!(bash_export.status.success());
}

#[test]
fn shell_print_cache_path_outputs_valid_path() {
    let cache_dir = tempfile::tempdir().unwrap();

    let flake_reference = "github:owner/repo";

    let mut cmd = Command::new(cargo::cargo_bin!());
    cmd.args([
        "shell",
        "print-cache-path",
        "--flake-reference",
        flake_reference,
        "--cache-dir",
        &cache_dir.path().to_string_lossy(),
    ]);

    cmd.assert().success().stdout(predicate::eq(
        cache_dir
            .path()
            .join(format!("{:x}\n", Sha1::digest(flake_reference)))
            .to_string_lossy(),
    ));
}

#[test]
fn shell_export_with_empty_config_and_no_flake_references() {
    let work_dir = tempfile::tempdir().unwrap();
    let bin_dir = work_dir.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let nix_file = bin_dir.join("nix");

    let bash_path = env::var("NIX_BIN_BASH").unwrap_or_else(|_| String::from("/bin/bash"));
    let nix_file_content = format!(
        r#"#! {bash_path}

if [[ "$@" == "--extra-experimental-features nix-command flakes --version" ]]; then
  echo "nix (Nix) 2.30.0"
fi

exit 0
"#
    );
    fs::write(&nix_file, nix_file_content).unwrap();
    fs::set_permissions(&nix_file, fs::Permissions::from_mode(0o755)).unwrap();

    let original_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), original_path);

    let mut cmd = Command::new(cargo::cargo_bin!());
    cmd.args(["shell", "export", "bash"]).env("PATH", new_path);

    cmd.assert().success().stdout(predicate::eq(""));
}

#[test]
fn test_shell_export_state_init_update_and_reset() {
    let work_dir = tempfile::tempdir().unwrap();
    let cache_dir = tempfile::tempdir_in(work_dir.path()).unwrap();
    let config_file = work_dir.path().join("config.toml");
    fs::write(
        &config_file,
        toml::to_string_pretty(&toml::toml! {
          [[entries]]
          pattern = "^/some/dir(/.*)?"
          flake_reference = "github:owner/repo"
          [[entries]]
          pattern = "^/some/other/dir(/.*)?"
          flake_reference = "github:other_github_owner/repo"
        })
        .unwrap(),
    )
    .unwrap();
    let bin_dir = work_dir.path().join("bin");
    fs::create_dir(&bin_dir).unwrap();
    let nix_file = bin_dir.join("nix");

    let bash_path = env::var("NIX_BIN_BASH").unwrap_or_else(|_| String::from("/bin/bash"));
    let profile_rc_content = "export FAKE_VAR=true;";
    let nix_file_content = format!(
        r#"#! {bash_path}

if [[ "$@" == "--extra-experimental-features nix-command flakes --version" ]]; then
    echo "nix (Nix) 2.30.0"
elif [[ "$@" == "--extra-experimental-features nix-command flakes print-dev-env"* ]]; then
rc="{profile_rc_content}"
for ((i=0; i<$#; i++)); do
    if [[ "${{@:$i:1}}" == "--profile" ]]; then
        profile_path="${{@:$((i+1)):1}}"
        echo "$rc" > "$profile_path"
        break
    fi
done
echo "$rc"
elif [[ "$@" == "--extra-experimental-features nix-command flakes build"* ]]; then
for ((i=0; i<$#; i++)); do
    if [[ "${{@:$i:1}}" == "--out-link" ]]; then
        link_path="${{@:$((i+1)):1}}"
        installable="${{@:$((i+2)):1}}"
        mkdir -p "$(dirname "$link_path")"
        ln -sf "$installable" "$link_path"
        break
    fi
done
elif [[ "$@" == "--extra-experimental-features nix-command flakes flake archive"* ]]; then
echo '{{ "inputs": {{ "nixpkgs": {{ "inputs": {{}}, "path": "/nix/store/yfzmnk75f009yb7b542kf4r7qaqq9kid-source" }} }} }}'
fi

exit 0
"#
    );
    fs::write(&nix_file, nix_file_content).unwrap();
    fs::set_permissions(&nix_file, fs::Permissions::from_mode(0o755)).unwrap();

    let original_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), original_path);

    {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "shell",
            "export",
            "bash",
            "--config-path",
            &config_file.to_string_lossy(),
            "--cache-dir",
            &cache_dir.path().to_string_lossy(),
            "--current-dir",
            "/no-match-path",
        ])
        .env("PATH", &new_path);

        let no_match_output = cmd.output().unwrap();

        assert!(no_match_output.status.success());

        let no_match_shell_export = String::from_utf8_lossy(&no_match_output.stdout);

        assert_eq!(no_match_shell_export, "");
    }

    let initial_shell_export = {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "shell",
            "export",
            "bash",
            "--config-path",
            &config_file.to_string_lossy(),
            "--cache-dir",
            &cache_dir.path().to_string_lossy(),
            "--current-dir",
            "/some/dir",
        ])
        .env("PATH", &new_path);

        let initial_output = cmd.output().unwrap();

        assert!(initial_output.status.success());

        String::from(String::from_utf8_lossy(&initial_output.stdout))
    };

    assert_eq!(
        initial_shell_export
            .split('\n')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>(),
        vec![
            "export FAKE_VAR=true;",
            "export ENVOLUNTARY_ENV_STATE=KLUv/QQ4dQMArAYAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9yZXBvIl0sImVudl92YXJzX3Jlc2V0Ijp7IkZBS0VfVkFSIjpudWxsLCJFTlZPTFVOVEFSWV9FTlZfU1RBVEUiOm51bGx9fQCbvTM7;",
        ]
    );

    let update_shell_export = {
        let initial_env_vars = get_env_vars_from_bash(
            BashSource::Script(initial_shell_export.into()),
            Some(EnvVars::from_iter([("PATH".to_string(), new_path.clone())])),
        )
        .unwrap();
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "shell",
            "export",
            "bash",
            "--config-path",
            &config_file.to_string_lossy(),
            "--cache-dir",
            &cache_dir.path().to_string_lossy(),
            "--current-dir",
            "/some/other/dir",
        ])
        .env("PATH", &new_path)
        .envs(initial_env_vars.iter());

        let update_output = cmd.output().unwrap();

        assert!(update_output.status.success());

        String::from(String::from_utf8_lossy(&update_output.stdout))
    };

    assert_eq!(
        update_shell_export
            .split('\n')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>(),
        vec![
            "unset FAKE_VAR;",
            "unset ENVOLUNTARY_ENV_STATE;",
            "export FAKE_VAR=true;",
            "export ENVOLUNTARY_ENV_STATE=$'KLUv/QQ4HQQAHAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1YjpvdGhlcl9fb3duZXIvcmVwbyJdLCJlbnZfdmFyc19yZXNldCI6eyJGQUtFX1ZBUiI6bnVsbCwiRU5WT0xVTlRBUllfRU5WX1NUQVRFIjpudWxsfX0BqBDj//0Qww8Qow+DMUZWbCwq';"
        ]
    );

    let update_env_vars = get_env_vars_from_bash(
        BashSource::Script(update_shell_export.into()),
        Some(EnvVars::from_iter([("PATH".to_string(), new_path.clone())])),
    )
    .unwrap();

    {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "shell",
            "export",
            "bash",
            "--config-path",
            &config_file.to_string_lossy(),
            "--cache-dir",
            &cache_dir.path().to_string_lossy(),
            "--current-dir",
            "/some/other/dir",
        ])
        .env("PATH", &new_path)
        .envs(update_env_vars.iter());

        let no_update_output = cmd.output().unwrap();

        assert!(no_update_output.status.success());

        let no_update_shell_export =
            String::from(String::from_utf8_lossy(&no_update_output.stdout));

        assert_eq!(no_update_shell_export, "");
    }

    let reset_shell_export = {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "shell",
            "export",
            "bash",
            "--config-path",
            &config_file.to_string_lossy(),
            "--cache-dir",
            &cache_dir.path().to_string_lossy(),
            "--current-dir",
            "/",
        ])
        .env("PATH", &new_path)
        .envs(update_env_vars.iter());

        let reset_output = cmd.output().unwrap();

        assert!(reset_output.status.success());

        String::from(String::from_utf8_lossy(&reset_output.stdout))
    };

    assert_eq!(
        reset_shell_export
            .split('\n')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>(),
        vec!["unset FAKE_VAR;", "unset ENVOLUNTARY_ENV_STATE;"]
    );
}
