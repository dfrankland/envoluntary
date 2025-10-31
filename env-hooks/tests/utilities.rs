use std::env;
use std::os::unix::fs::PermissionsExt;
use std::{collections::HashSet, fs};

use env_hooks::{
    BashSource, EnvVars, EnvVarsState, get_env_vars_from_bash, get_env_vars_from_current_process,
    get_env_vars_reset, get_old_env_vars_to_be_updated, merge_delimited_env_var,
    remove_ignored_env_vars,
};

#[test]
fn get_old_env_vars_to_be_updated_finds_changed_vars() {
    let old_vars = EnvVars::from_iter([
        ("VAR1".to_string(), "old_value".to_string()),
        ("VAR2".to_string(), "unchanged".to_string()),
        ("VAR3".to_string(), "old".to_string()),
    ]);

    let new_vars = EnvVars::from_iter([
        ("VAR1".to_string(), "new_value".to_string()),
        ("VAR2".to_string(), "unchanged".to_string()),
        ("VAR3".to_string(), "old".to_string()),
    ]);

    let result = get_old_env_vars_to_be_updated(old_vars, &new_vars);

    assert_eq!(
        result,
        EnvVars::from_iter([("VAR1".to_string(), "old_value".to_string())])
    );
}

#[test]
fn get_old_env_vars_to_be_updated_ignores_new_vars() {
    let old_vars = EnvVars::from_iter([("VAR1".to_string(), "value1".to_string())]);

    let new_vars = EnvVars::from_iter([
        ("VAR1".to_string(), "value1".to_string()),
        ("VAR2".to_string(), "value2".to_string()),
    ]);

    let result = get_old_env_vars_to_be_updated(old_vars, &new_vars);

    assert!(result.is_empty());
}

#[test]
fn get_old_env_vars_to_be_updated_empty_old_vars() {
    let old_vars = EnvVars::new();
    let new_vars = EnvVars::new();

    let result = get_old_env_vars_to_be_updated(old_vars, &new_vars);

    assert!(result.is_empty());
}

#[test]
fn get_env_vars_reset_returns_state_to_old_env_vars() {
    let old_env_vars = EnvVars::from_iter([
        ("VAR1".to_string(), "old1".to_string()),
        ("VAR2".to_string(), "old2".to_string()),
    ]);

    let new_vars = HashSet::from_iter(["VAR1".to_string(), "VAR3".to_string()]);

    let result = get_env_vars_reset(old_env_vars, new_vars, "STATE_VAR".to_string());

    assert_eq!(
        result,
        EnvVarsState::from_iter([
            ("VAR1".to_string(), Some("old1".to_string())),
            ("VAR3".to_string(), None),
            ("STATE_VAR".to_string(), None),
        ])
    );
}

#[test]
fn get_env_vars_from_current_process_returns_current_env() {
    let result = get_env_vars_from_current_process();
    let path = env::var("PATH").ok();
    assert!(path.is_some());
    assert_eq!(result.get("PATH"), path.as_ref());
}

#[test]
fn merge_delimited_values_combines_paths_and_preserves_order_with_new_paths_in_front() {
    {
        let mut new_env_vars = EnvVars::new();
        merge_delimited_env_var("PATH", ':', ':', &EnvVars::new(), &mut new_env_vars);
        assert!(new_env_vars.is_empty());
    }

    let mut new_env_vars =
        EnvVars::from_iter([("PATH".to_string(), "/home/user/bin:/usr/bin".to_string())]);
    merge_delimited_env_var(
        "PATH",
        ':',
        ' ',
        &EnvVars::from_iter([("PATH".to_string(), "/usr/bin:/usr/local/bin".to_string())]),
        &mut new_env_vars,
    );

    assert_eq!(
        new_env_vars,
        EnvVars::from_iter([(
            "PATH".to_string(),
            "/home/user/bin /usr/bin /usr/local/bin".to_string()
        )])
    );
}

#[test]
fn env_vars_into_env_vars_state_conversion() {
    assert_eq!(
        EnvVarsState::from(EnvVars::from_iter([
            ("VAR1".to_string(), "value1".to_string()),
            ("VAR2".to_string(), "value2".to_string()),
        ])),
        EnvVarsState::from_iter([
            ("VAR1".to_string(), Some("value1".to_string())),
            ("VAR2".to_string(), Some("value2".to_string())),
        ])
    );
}

#[test]
fn test_getting_env_vars_from_bash() {
    let tempdir = tempfile::tempdir().unwrap();
    let bash_script_path = tempdir.path().join("my_bash_script.sh");
    let bash_path = env::var("NIX_BIN_BASH").unwrap_or_else(|_| String::from("/bin/bash"));
    fs::write(
        &bash_script_path,
        format!("#! {bash_path}\nexport TEST_VAR=true"),
    )
    .unwrap();
    fs::set_permissions(&bash_script_path, fs::Permissions::from_mode(0o755)).unwrap();

    let old_path = env::var("PATH").unwrap();

    let mut new_env_vars = get_env_vars_from_bash(
        BashSource::File(bash_script_path),
        Some(EnvVars::from_iter([(
            String::from("PATH"),
            old_path.clone(),
        )])),
    )
    .unwrap();
    remove_ignored_env_vars(&mut new_env_vars);

    assert_eq!(new_env_vars.get("PATH").unwrap(), &old_path);
    new_env_vars.shift_remove("PATH");

    assert_eq!(
        new_env_vars,
        EnvVars::from_iter([(String::from("TEST_VAR"), String::from("true"))])
    );
}
