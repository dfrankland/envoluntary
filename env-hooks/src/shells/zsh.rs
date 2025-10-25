use std::collections::HashSet;

use bstr::{B, BString, ByteSlice};
use shell_quote::Zsh;

use crate::EnvVarsState;

const ZSH_HOOK: &str = r#"
    _{{.HookPrefix}}_hook() {
        vars="$({{.ExportCommand}})"
        trap -- '' SIGINT
        eval "$vars"
        trap - SIGINT
    }
    typeset -ag precmd_functions
    if (( ! ${precmd_functions[(I)_{{.HookPrefix}}_hook]} )); then
        precmd_functions=(_{{.HookPrefix}}_hook $precmd_functions)
    fi
    typeset -ag chpwd_functions
    if (( ! ${chpwd_functions[(I)_{{.HookPrefix}}_hook]} )); then
        chpwd_functions=(_{{.HookPrefix}}_hook $chpwd_functions)
    fi
"#;

pub fn hook(hook_prefix: impl AsRef<[u8]>, export_command: impl AsRef<[u8]>) -> BString {
    BString::from(ZSH_HOOK)
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
    let script = bstr::join(" ", [B("export"), &Zsh::quote_vec(key)]);
    let value = Zsh::quote_vec(value);
    bstr::concat([&bstr::join("=", [script, value]), B(";")]).into()
}

fn unset_var(key: &str) -> BString {
    bstr::concat([&bstr::join(" ", [B("unset"), &Zsh::quote_vec(key)]), B(";")]).into()
}
