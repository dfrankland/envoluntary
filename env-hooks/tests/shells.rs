use std::collections::HashSet;

use bstr::ByteSlice;
use env_hooks::{
    EnvVarsState,
    shells::{bash, fish, json, nushell, zsh},
};
use once_cell::sync::Lazy;

static TEST_ENV_VARS: Lazy<EnvVarsState> = Lazy::new(|| {
    EnvVarsState::from_iter(vec![
        ("SIMPLE".to_string(), Some("value".to_string())),
        ("TO_REMOVE".to_string(), None),
        (
            "WITH_SPACES".to_string(),
            Some("value with spaces".to_string()),
        ),
        ("DOLLAR".to_string(), Some("$VAR".to_string())),
        ("EMPTY".to_string(), Some("".to_string())),
        (
            "PATH".to_string(),
            Some("/usr/bin:/usr/local/bin".to_string()),
        ),
        ("VAR123".to_string(), Some("numeric".to_string())),
        ("_PRIVATE".to_string(), Some("private".to_string())),
        (
            "MULTI_LINE_VAR".to_string(),
            Some("\nHello,\nWorld!\n".to_string()),
        ),
    ])
});

#[test]
fn bash_export_set_unset_and_special_vars() {
    assert_eq!(bash::export(EnvVarsState::new(), None), "");

    let result = bash::export(TEST_ENV_VARS.clone(), None).to_string();
    let lines = result.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>();

    assert_eq!(
        lines,
        vec![
            "export SIMPLE=value;",
            "unset TO_REMOVE;",
            "export WITH_SPACES=$'value with spaces';",
            "export DOLLAR=$'$VAR';",
            "export EMPTY='';",
            "export PATH=$'/usr/bin:/usr/local/bin';",
            "export VAR123=numeric;",
            "export _PRIVATE=private;",
            "export MULTI_LINE_VAR=$'\\nHello,\\nWorld!\\n';"
        ]
    );
}

#[test]
fn bash_hook_templated() {
    let result = bash::hook("myapp", "myapp export bash").to_string();
    assert!(!result.contains("{{."));
}

#[test]
fn zsh_export_set_unset_and_special_vars() {
    assert_eq!(zsh::export(EnvVarsState::new(), None).to_str().unwrap(), "");

    let result = zsh::export(TEST_ENV_VARS.clone(), None).to_string();
    let lines = result.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>();

    assert_eq!(
        lines,
        vec![
            "export SIMPLE=value;",
            "unset TO_REMOVE;",
            "export WITH_SPACES=$'value with spaces';",
            "export DOLLAR=$'$VAR';",
            "export EMPTY='';",
            "export PATH=$'/usr/bin:/usr/local/bin';",
            "export VAR123=numeric;",
            "export _PRIVATE=private;",
            "export MULTI_LINE_VAR=$'\\nHello,\\nWorld!\\n';"
        ]
    );
}

#[test]
fn zsh_hook_templated() {
    let result = zsh::hook("myapp", "myapp export zsh").to_string();
    assert!(!result.contains("{{."));
}

#[test]
fn nushell_export_set_unset_and_special_vars() {
    assert_eq!(nushell::export(EnvVarsState::new()).to_str().unwrap(), "{}");

    let result = nushell::export(TEST_ENV_VARS.clone()).to_string();

    assert_eq!(
        result,
        "{\"SIMPLE\":\"value\",\"TO_REMOVE\":null,\"WITH_SPACES\":\"value with spaces\",\"DOLLAR\":\"$VAR\",\"EMPTY\":\"\",\"PATH\":\"/usr/bin:/usr/local/bin\",\"VAR123\":\"numeric\",\"_PRIVATE\":\"private\",\"MULTI_LINE_VAR\":\"\\nHello,\\nWorld!\\n\"}",
    );
}

#[test]
fn nushell_hook_templated() {
    let result = nushell::hook("myapp export nushell").to_string();
    assert!(!result.contains("{{."));
}

#[test]
fn fish_export_set_unset_and_special_vars() {
    assert_eq!(
        fish::export(EnvVarsState::new(), None).to_str().unwrap(),
        ""
    );

    let result = fish::export(TEST_ENV_VARS.clone(), None).to_string();
    let lines = result.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>();

    assert_eq!(
        lines,
        vec![
            r#"set -x -g SIMPLE value;"#,
            r#"set -e -g TO_REMOVE;"#,
            r#"set -x -g WITH_SPACES value' with spaces';"#,
            r#"set -x -g DOLLAR '$VAR';"#,
            r#"set -x -g EMPTY '';"#,
            r#"set -x -g PATH /usr/bin':/usr/local/bin';"#,
            r#"set -x -g VAR123 numeric;"#,
            r#"set -x -g _PRIVATE private;"#,
            r#"set -x -g MULTI_LINE_VAR \nHello,\nWorld'!'\n;"#,
        ]
    );
}

#[test]
fn fish_export_delimited_variables() {
    let env_vars = EnvVarsState::from_iter(vec![(
        "PATH".to_string(),
        Some("/usr/bin:/usr/local/bin:/home/user/bin".to_string()),
    )]);

    let result = fish::export(env_vars.clone(), None).to_string();
    let lines = result.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec!["set -x -g PATH /usr/bin':/usr/local/bin:/home/user/bin';"]
    );

    let mut delim_vars = HashSet::new();
    delim_vars.insert("PATH".to_string());
    let result = fish::export(env_vars.clone(), Some(&delim_vars)).to_string();
    let lines = result.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>();
    assert_eq!(
        lines,
        vec!["set -x -g PATH /usr/bin /usr/local/bin /home/user/bin;"]
    );
}

#[test]
fn fish_hook_templated() {
    let result = fish::hook("myapp", "myapp export fish").to_string();
    assert!(!result.contains("{{."));
}

#[test]
fn json_export_set_unset_and_special_vars() {
    assert_eq!(
        json::export(EnvVarsState::new(), None).to_str().unwrap(),
        "{}"
    );

    let result = json::export(TEST_ENV_VARS.clone(), None).to_string();
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();

    assert_eq!(
        parsed,
        serde_json::json!({
            "SIMPLE": "value",
            "TO_REMOVE": null,
            "WITH_SPACES": "value with spaces",
            "DOLLAR": "$VAR",
            "EMPTY": "",
            "PATH": "/usr/bin:/usr/local/bin",
            "VAR123": "numeric",
            "_PRIVATE": "private",
            "MULTI_LINE_VAR": "\nHello,\nWorld!\n"
        })
    );
}
