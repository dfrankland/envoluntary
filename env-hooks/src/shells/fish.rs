use std::{collections::HashSet, ffi::OsString};

use shell_quote::{Fish, QuoteExt};

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

pub fn hook(hook_prefix: &str, export_command: &str) -> OsString {
    OsString::from(
        FISH_HOOK
            .replace("{{.HookPrefix}}", hook_prefix)
            .replace("{{.ExportCommand}}", export_command),
    )
}

pub fn export(
    env_vars_state: EnvVarsState,
    semicolon_delimited_env_vars: Option<&HashSet<String>>,
) -> OsString {
    env_vars_state
        .iter()
        .fold(OsString::new(), |mut acc, (key, state)| {
            acc.push(if let Some(value) = state {
                export_var(key, value, semicolon_delimited_env_vars)
            } else {
                unset_var(key)
            });
            acc.push("\n");
            acc
        })
}

fn export_var(
    key: &str,
    value: &str,
    semicolon_delimited_env_vars: Option<&HashSet<String>>,
) -> OsString {
    let mut script = OsString::from("set -x -g ");
    script.push_quoted(Fish, key);
    if let Some(sdev) = semicolon_delimited_env_vars
        && sdev.contains(key)
    {
        value.split(':').for_each(|value_part| {
            script.push(" ");
            script.push_quoted(Fish, value_part);
        });
    } else {
        script.push(" ");
        script.push_quoted(Fish, value);
    }
    script.push(";");
    script
}

fn unset_var(key: &str) -> OsString {
    let mut script = OsString::from("set -e -g ");
    script.push_quoted(Fish, key);
    script.push(";");
    script
}
