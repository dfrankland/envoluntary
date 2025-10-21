use std::collections::HashSet;

use bstr::{B, BString, ByteSlice};
use shell_quote::Fish;

use crate::EnvVarsState;

const FISH_HOOK: &str = r#"
    function __{{.HookPrefix}}_export_eval --on-event fish_prompt;
        {{.ExportCommand}} | source;

        if test "${{.HookPrefix}}_fish_mode" != "disable_arrow";
            function __{{.HookPrefix}}_cd_hook --on-variable PWD;
                if test "${{.HookPrefix}}_fish_mode" = "eval_after_arrow";
                    set -g __{{.HookPrefix}}_export_again 0;
                else;
                    {{.ExportCommand}} | source;
                end;
            end;
        end;
    end;

    function __{{.HookPrefix}}_export_eval_2 --on-event fish_preexec;
        if set -q __{{.HookPrefix}}_export_again;
            set -e __{{.HookPrefix}}_export_again;
            {{.ExportCommand}} | source;
            echo;
        end;

        functions --erase __{{.HookPrefix}}_cd_hook;
    end;
"#;

pub fn hook(hook_prefix: impl AsRef<[u8]>, export_command: impl AsRef<[u8]>) -> BString {
    BString::from(FISH_HOOK)
        .replace("{{.HookPrefix}}", hook_prefix)
        .replace("{{.ExportCommand}}", export_command)
        .into()
}

pub fn export(
    env_vars_state: EnvVarsState,
    semicolon_delimited_env_vars: Option<&HashSet<String>>,
) -> BString {
    let exports = env_vars_state
        .iter()
        .map(|(key, state)| {
            if let Some(value) = state {
                export_var(key, value, semicolon_delimited_env_vars)
            } else {
                unset_var(key)
            }
        })
        .collect::<Vec<_>>();
    bstr::join("\n", exports).into()
}

fn export_var(
    key: &str,
    value: &str,
    semicolon_delimited_env_vars: Option<&HashSet<String>>,
) -> BString {
    let script = bstr::join(" ", [B("set -x -g"), &Fish::quote_vec(key)]);
    let value = if let Some(sdev) = semicolon_delimited_env_vars
        && sdev.contains(key)
    {
        let value_parts = value.split(':').map(Fish::quote_vec).collect::<Vec<_>>();
        bstr::join(" ", value_parts)
    } else {
        Fish::quote_vec(value)
    };
    bstr::concat([&bstr::join(" ", [script, value]), B(";")]).into()
}

fn unset_var(key: &str) -> BString {
    bstr::concat([
        &bstr::join(" ", [B("set -e -g"), &Fish::quote_vec(key)]),
        B(";"),
    ])
    .into()
}
