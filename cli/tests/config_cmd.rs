use std::{env, fs, os::unix::fs::PermissionsExt};

use assert_cmd::{Command, cargo};
use predicates::prelude::*;

#[test]
fn config_print_path_outputs_valid_path() {
    let mut cmd = Command::new(cargo::cargo_bin!());
    cmd.args(["config", "print-path"])
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", "/some/path");

    cmd.assert().success().stdout(predicate::eq(
        "/some/path/.config/envoluntary/config.toml\n",
    ));

    cmd.env("XDG_CONFIG_HOME", "/some/other/path/.config");

    cmd.assert().success().stdout(predicate::eq(
        "/some/other/path/.config/envoluntary/config.toml\n",
    ));
}

#[test]
fn config_edit_executes_editor_with_config_path() {
    let editor_dir = tempfile::tempdir().unwrap();
    let (_, editor_file_path) = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o755))
        .tempfile()
        .unwrap()
        .keep()
        .unwrap();
    let bash_path = env::var("NIX_BIN_BASH").unwrap_or_else(|_| String::from("/bin/bash"));
    let log_file = editor_dir.path().join("log.txt");
    fs::write(
        &editor_file_path,
        format!(
            r#"#! {bash_path}
echo "$@" > {log_file};
exit 0;"#,
            log_file = log_file.to_string_lossy()
        ),
    )
    .unwrap();

    {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "config",
            "edit",
            "--editor-program",
            &editor_file_path.to_string_lossy(),
        ])
        .env_remove("EDITOR")
        .env("XDG_CONFIG_HOME", "/some/dir/.config");
        cmd.assert().success();
        assert_eq!(
            fs::read_to_string(&log_file).unwrap(),
            "/some/dir/.config/envoluntary/config.toml\n"
        );
    }

    {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args(["config", "edit"])
            .env("EDITOR", editor_file_path.to_string_lossy().to_string())
            .env("XDG_CONFIG_HOME", "/some/other/dir/.config");
        cmd.assert().success();
        assert_eq!(
            fs::read_to_string(&log_file).unwrap(),
            "/some/other/dir/.config/envoluntary/config.toml\n"
        );
    }
}

#[test]
fn config_add_entry_with_custom_config_path() {
    let config_dir = tempfile::tempdir().unwrap();
    let config_path = config_dir.path().join("config.toml");
    fs::write(&config_path, "").unwrap();

    {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "config",
            "print-matching-entries",
            "/some/path",
            "--config-path",
            &config_path.to_string_lossy(),
        ]);

        cmd.assert().success().stdout(predicate::eq("[]\n"));
    }

    {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "config",
            "add-entry",
            "^/home/test",
            "github:owner/repo",
            "--pattern-adjacent",
            ".*/package.json",
            "--impure",
            "true",
            "--config-path",
            &config_path.to_string_lossy(),
        ]);

        cmd.assert().success();

        let config_string = fs::read_to_string(&config_path).unwrap();
        let config: toml::Value = toml::from_str(&config_string).unwrap();

        assert_eq!(
            config,
            toml::toml! {
                [[entries]]
                pattern = "^/home/test"
                flake_reference = "github:owner/repo"
                pattern_adjacent = ".*/package.json"
                impure = true
            }
            .into()
        )
    }

    {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "config",
            "add-entry",
            ".*",
            "github:owner/repo",
            "--config-path",
            &config_path.to_string_lossy(),
        ]);

        cmd.assert().success();

        let config_string = fs::read_to_string(&config_path).unwrap();
        let config: toml::Value = toml::from_str(&config_string).unwrap();

        assert_eq!(
            config,
            toml::toml! {
                [[entries]]
                pattern = "^/home/test"
                flake_reference = "github:owner/repo"
                pattern_adjacent = ".*/package.json"
                impure = true

                [[entries]]
                pattern = ".*"
                flake_reference = "github:owner/repo"
            }
            .into()
        )
    }

    {
        let mut cmd = Command::new(cargo::cargo_bin!());
        cmd.args([
            "config",
            "print-matching-entries",
            "/some/path",
            "--config-path",
            &config_path.to_string_lossy(),
        ]);

        cmd.assert().success();

        let json_output: serde_json::Value =
            serde_json::from_slice(&cmd.output().unwrap().stdout).unwrap();

        assert_eq!(
            json_output,
            serde_json::json!([{
                "pattern": ".*",
                "flake_reference": "github:owner/repo",
                "pattern_adjacent": null,
                "impure": null
            }])
        )
    }
}
