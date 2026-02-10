use bstr::{BString, ByteSlice};

use crate::EnvVarsState;

const NUSHELL_HOOK: &str = r#"
$env.config.hooks.env_change.PWD = (
    $env.config.hooks.env_change | get --optional PWD | default [] | append { ||
        {{.ExportCommand}} | from json | default {} | load-env
    }
)

$env.config.hooks.pre_execution = (
    $env.config.hooks.pre_execution | append { ||
        {{.ExportCommand}} | from json | default {} | load-env
    }
)
"#;

pub fn hook(export_command: impl AsRef<[u8]>) -> BString {
    BString::from(NUSHELL_HOOK)
        .replace("{{.ExportCommand}}", export_command)
        .into()
}

pub fn export(env_vars_state: EnvVarsState) -> BString {
    let json = serde_json::to_string(&env_vars_state).unwrap_or_else(|_| String::from("{}"));
    if json.trim().is_empty() {
        String::from("{}").into()
    } else {
        json.into()
    }
}
