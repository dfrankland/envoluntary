use std::{
    env, fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    process::{self, Output},
};

use assert_cmd::{Command, cargo};
use env_hooks::{BashSource, EnvVars, get_env_vars_from_bash};
use predicates::prelude::*;
use sha1::{Digest, Sha1};
use tempfile::TempDir;

fn test_evaluable_syntax(shell_name: &str, shell_cmd: &str) {
    let mut cmd = Command::new(cargo::cargo_bin!());
    cmd.args(["shell", "hook", shell_name]);

    let export_output = cmd.output().unwrap();
    assert!(export_output.status.success());

    let script = String::from_utf8_lossy(&export_output.stdout);

    assert!(!script.contains("{{."));

    let export = process::Command::new(shell_cmd)
        .arg("-c")
        .arg(script.as_ref())
        .output()
        .unwrap();

    assert!(export.status.success());
}

const NUSHELL_ENV_CHECK: &str = r#"
let json_data = '{{.JsonBlob}}' | from json

let invalid = ($json_data | items {|k, v| if ($env | get -o $k) != $v { $"($k): ($env | get -o $k) instead of ($v)" } } | compact)

if ($invalid | length) > 0 {
    error make { msg: $"Invalid environment variables: ($invalid | str join ', ')" }
} else {
    print "All variables correct in environment."
}
"#;

struct TestVals {
    pub work_dir: TempDir,
    pub cache_dir: TempDir,
    pub config_file: PathBuf,
    pub bin_dir: PathBuf,
    pub path: String,
}

fn build_test_vals() -> TestVals {
    let work_dir = tempfile::tempdir().unwrap();
    let cache_dir = tempfile::tempdir_in(work_dir.path()).unwrap();

    let config_file = setup_mock_config(work_dir.path());
    let bin_dir = setup_mock_nix_bin(work_dir.path());

    let original_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), original_path);
    TestVals {
        work_dir,
        cache_dir,
        config_file,
        bin_dir,
        path: new_path,
    }
}

fn load_nu_env_string(test_vals: &TestVals, current_dir: &str) -> String {
    format!(
            "envoluntary shell export nushell --config-path {config_path} --cache-dir {cache_dir} --current-dir {current_dir} | from json --objects | default {{}} | reduce --fold {{}} {{|row, acc| $acc | merge $row}}
 | load-env",
            config_path = &test_vals.config_file.to_string_lossy(),
            cache_dir = &test_vals.cache_dir.path().to_string_lossy(),
            current_dir = current_dir
        )
}

fn export_and_check(
    test_vals: &TestVals,
    current_dir: &str,
    home_dir: &str,
    extra_script: Option<&str>,
    expected_env: &serde_json::Value,
) -> Output {
    let mut cmd = Command::new("nu");
    let env_check_script = NUSHELL_ENV_CHECK.replace("{{.JsonBlob}}", &expected_env.to_string());
    let script = if let Some(ext_script) = extra_script {
        ext_script.to_owned() + ";\n" + &env_check_script
    } else {
        env_check_script
    };
    cmd.args([
        "-c",
        &format!(
            "{load_env};\n{script}",
            load_env = load_nu_env_string(&test_vals, current_dir),
            script = script
        ),
    ])
    .env("PATH", &test_vals.path)
    .env("HOME", home_dir);

    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn test_nushell_export_state_init_update_and_reset() {
    let test_vals = build_test_vals();

    // --- TEST CASES ---

    // Case 1: No matching path
    export_and_check(
        &test_vals,
        "/no-match-path",
        "/home",
        None,
        &serde_json::json!({
            "FAKE_VAR": null,
            "ENVOLUNTARY_ENV_STATE": null,
        }),
    );

    // Case 2: Initial export
    let expected_json = &serde_json::json!({
        "FAKE_VAR": "true",
        "ENVOLUNTARY_ENV_STATE": "KLUv/QQ4dQMArAYAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9yZXBvIl0sImVudl92YXJzX3Jlc2V0Ijp7IkZBS0VfVkFSIjpudWxsLCJFTlZPTFVOVEFSWV9FTlZfU1RBVEUiOm51bGx9fQCbvTM7"
    });
    export_and_check(&test_vals, "/some/dir", "/home", None, expected_json);

    // // Case 3: Update state
    let expected_update_json = &serde_json::json!({
        "FAKE_VAR": "true",
        "ENVOLUNTARY_ENV_STATE": "KLUv/QQ4HQQAHAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1YjpvdGhlcl9fb3duZXIvcmVwbyJdLCJlbnZfdmFyc19yZXNldCI6eyJGQUtFX1ZBUiI6bnVsbCwiRU5WT0xVTlRBUllfRU5WX1NUQVRFIjpudWxsfX0BqBDj//0Qww8Qow+DMUZWbCwq",
    });

    export_and_check(
        &test_vals,
        "/some/dir",
        "/home",
        Some(&load_nu_env_string(&test_vals, "/home/some/other/dir")),
        expected_update_json,
    );

    // Case 4: Export in a dir, re-export in same dir
    export_and_check(
        &test_vals,
        "/home/some/other/dir",
        "/home",
        Some(&load_nu_env_string(&test_vals, "/home/some/other/dir")),
        &serde_json::json!({}),
    );

    // Case 5: Reset state
    export_and_check(
        &test_vals,
        "/home/some/other/dir",
        "/home",
        Some(&load_nu_env_string(&test_vals, "/")),
        &serde_json::json!({ "ENVOLUNTARY_ENV_STATE": null }),
    );

    // Case 6: Adjacent pattern matching
    export_and_check(
        &test_vals,
        &test_vals.bin_dir.to_string_lossy(),
        "/home",
        None,
        &serde_json::json!({ "ENVOLUNTARY_ENV_STATE": null }),
    );

    fs::File::create_new(test_vals.work_dir.path().join(".supercooltool")).unwrap();
    export_and_check(
        &test_vals,
        &test_vals.bin_dir.to_string_lossy(),
        "/home",
        None,
        &serde_json::json!({
            "FAKE_VAR": "true",
            "ENVOLUNTARY_ENV_STATE": "KLUv/QQ4zQMAXAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9zdXBlcl9jb29sX3Rvb2wiXSwiZW52X3ZhcnNfcmVzZXQiOnsiRkFLRV9WQVIiOm51bGwsIkVOVk9MVU5UQVJZX0VOVl9TVEFURSI6bnVsbH19ACCjTjk="
        }),
    );

    // Case 7: Home directory adjacent pattern
    let home_dir = tempfile::tempdir().unwrap();
    let home_path = home_dir.path().to_string_lossy();
    fs::File::create_new(home_dir.path().join(".awesometool")).unwrap();

    export_and_check(
        &test_vals,
        &home_path,
        &home_path,
        None,
        &serde_json::json!({
            "FAKE_VAR": "true",
            "ENVOLUNTARY_ENV_STATE": "KLUv/QQ4tQMALAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9hd2Vzb21lX3Rvb2wiXSwiZW52X3ZhcnNfcmVzZXQiOnsiRkFLRV9WQVIiOm51bGwsIkVOVk9MVU5UQVJZX0VOVl9TVEFURSI6bnVsbH19AB6hxkc="
        }),
    );
}

#[test]
fn shell_hook_bash_produces_evaluable_shell_syntax() {
    test_evaluable_syntax("bash", "bash");
}

#[test]
fn shell_hook_fish_produces_evaluable_shell_syntax() {
    test_evaluable_syntax("fish", "fish");
}

#[test]
fn shell_hook_nu_produces_evaluable_shell_syntax() {
    test_evaluable_syntax("nushell", "nu");
}

#[test]
fn shell_hook_zsh_produces_evaluable_shell_syntax() {
    test_evaluable_syntax("zsh", "zsh");
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
fn test_bash_export_state_init_update_and_reset() {
    let work_dir = tempfile::tempdir().unwrap();
    let cache_dir = tempfile::tempdir_in(work_dir.path()).unwrap();

    // 1. Setup mock environment
    let config_file = setup_mock_config(work_dir.path());
    let bin_dir = setup_mock_nix_bin(work_dir.path());

    let original_path = env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", bin_dir.display(), original_path);

    // 2. Helper closure to run the shell export command
    let run_export = |current_dir: &str, home_dir: &str, env_vars: Option<&EnvVars>| -> String {
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
            current_dir,
        ])
        .env("PATH", &new_path)
        .env("HOME", home_dir);

        if let Some(envs) = env_vars {
            cmd.envs(envs.iter());
        }

        let output = cmd.output().unwrap();
        assert!(
            output.status.success(),
            "Command failed in dir: {}",
            current_dir
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    // --- TEST CASES ---

    // Case 1: No matching path
    let no_match_out = run_export("/no-match-path", "/home", None);
    assert_eq!(no_match_out, "");

    // Case 2: Initial export
    let initial_export = run_export("/some/dir", "/home", None);
    assert_output_lines(
        &initial_export,
        &[
            "export FAKE_VAR=true;",
            "export ENVOLUNTARY_ENV_STATE=KLUv/QQ4dQMArAYAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9yZXBvIl0sImVudl92YXJzX3Jlc2V0Ijp7IkZBS0VfVkFSIjpudWxsLCJFTlZPTFVOVEFSWV9FTlZfU1RBVEUiOm51bGx9fQCbvTM7;",
        ],
    );

    // Case 3: Update state
    let initial_env_vars = get_env_vars_from_bash(
        BashSource::Script(initial_export.into()),
        Some(EnvVars::from_iter([("PATH".to_string(), new_path.clone())])),
    )
    .unwrap();

    let update_export = run_export("/home/some/other/dir", "/home", Some(&initial_env_vars));
    assert_output_lines(
        &update_export,
        &[
            "unset FAKE_VAR;",
            "unset ENVOLUNTARY_ENV_STATE;",
            "export FAKE_VAR=true;",
            "export ENVOLUNTARY_ENV_STATE=$'KLUv/QQ4HQQAHAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1YjpvdGhlcl9fb3duZXIvcmVwbyJdLCJlbnZfdmFyc19yZXNldCI6eyJGQUtFX1ZBUiI6bnVsbCwiRU5WT0xVTlRBUllfRU5WX1NUQVRFIjpudWxsfX0BqBDj//0Qww8Qow+DMUZWbCwq';",
        ],
    );

    // Case 4: No update needed
    let update_env_vars = get_env_vars_from_bash(
        BashSource::Script(update_export.into()),
        Some(EnvVars::from_iter([("PATH".to_string(), new_path.clone())])),
    )
    .unwrap();

    let no_update_export = run_export("/home/some/other/dir", "/home", Some(&update_env_vars));
    assert_eq!(no_update_export, "");

    // Case 5: Reset state
    let reset_export = run_export("/", "/home", Some(&update_env_vars));
    assert_output_lines(
        &reset_export,
        &["unset FAKE_VAR;", "unset ENVOLUNTARY_ENV_STATE;"],
    );

    // Case 6: Adjacent pattern matching
    let pattern_adjacent_no_match = run_export(&bin_dir.to_string_lossy(), "/home", None);
    assert_eq!(pattern_adjacent_no_match, "");

    fs::File::create_new(work_dir.path().join(".supercooltool")).unwrap();
    let pattern_adjacent_match = run_export(&bin_dir.to_string_lossy(), "/home", None);
    assert_output_lines(
        &pattern_adjacent_match,
        &[
            "export FAKE_VAR=true;",
            "export ENVOLUNTARY_ENV_STATE=$'KLUv/QQ4zQMAXAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9zdXBlcl9jb29sX3Rvb2wiXSwiZW52X3ZhcnNfcmVzZXQiOnsiRkFLRV9WQVIiOm51bGwsIkVOVk9MVU5UQVJZX0VOVl9TVEFURSI6bnVsbH19ACCjTjk=';",
        ],
    );

    // Case 7: Home directory adjacent pattern
    let home_dir = tempfile::tempdir().unwrap();
    let home_path = home_dir.path().to_string_lossy();
    fs::File::create_new(home_dir.path().join(".awesometool")).unwrap();

    let pattern_adjacent_home_export = run_export(&home_path, &home_path, None);
    assert_output_lines(
        &pattern_adjacent_home_export,
        &[
            "export FAKE_VAR=true;",
            "export ENVOLUNTARY_ENV_STATE=$'KLUv/QQ4tQMALAcAeyJmbGFrZV9yZWZlcmVuY2VzIjpbImdpdGh1Yjpvd25lci9hd2Vzb21lX3Rvb2wiXSwiZW52X3ZhcnNfcmVzZXQiOnsiRkFLRV9WQVIiOm51bGwsIkVOVk9MVU5UQVJZX0VOVl9TVEFURSI6bnVsbH19AB6hxkc=';",
        ],
    );
}

// --- HELPERS ---

fn assert_output_lines(output: &str, expected: &[&str]) {
    let lines: Vec<_> = output.split('\n').filter(|s| !s.is_empty()).collect();
    assert_eq!(lines, expected);
}

fn setup_mock_config(work_dir: &std::path::Path) -> std::path::PathBuf {
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

fn setup_mock_nix_bin(work_dir: &std::path::Path) -> std::path::PathBuf {
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
