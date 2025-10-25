use std::collections::HashSet;

use bstr::{B, BString, ByteSlice};
use shell_quote::Bash;

use crate::EnvVarsState;

const BASH_HOOK: &str = r#"
    _{{.HookPrefix}}_hook() {
        local previous_exit_status=$?;
        vars="$({{.ExportCommand}})";
        trap -- '' SIGINT;
        eval "$vars";
        trap - SIGINT;
        return $previous_exit_status;
    };
    if [[ ";${PROMPT_COMMAND[*]:-};" != *";_{{.HookPrefix}}_hook;"* ]]; then
        if [[ "$(declare -p PROMPT_COMMAND 2>&1)" == "declare -a"* ]]; then
            PROMPT_COMMAND=(_{{.HookPrefix}}_hook "${PROMPT_COMMAND[@]}")
        else
            PROMPT_COMMAND="_{{.HookPrefix}}_hook${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
        fi
    fi
"#;

pub fn hook(hook_prefix: impl AsRef<[u8]>, export_command: impl AsRef<[u8]>) -> BString {
    BString::from(BASH_HOOK)
        .replace("{{.HookPrefix}}", hook_prefix)
        .replace("{{.ExportCommand}}", export_command)
        .into()
}

pub fn export(
    env_vars_state: EnvVarsState,
    _semicolon_delimited_env_vars: Option<&HashSet<String>>,
) -> BString {
    let exports = env_vars_state
        .iter()
        .map(|(key, state)| {
            if let Some(value) = state {
                export_var(key, value)
            } else {
                unset_var(key)
            }
        })
        .collect::<Vec<_>>();
    bstr::join("\n", exports).into()
}

fn export_var(key: &str, value: &str) -> BString {
    let script = bstr::join(" ", [B("export"), &Bash::quote_vec(key)]);
    let value = Bash::quote_vec(value);
    bstr::concat([&bstr::join("=", [script, value]), B(";")]).into()
}

fn unset_var(key: &str) -> BString {
    bstr::concat([
        &bstr::join(" ", [B("unset"), &Bash::quote_vec(key)]),
        B(";"),
    ])
    .into()
}
