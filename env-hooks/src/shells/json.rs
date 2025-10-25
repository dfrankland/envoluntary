use std::collections::HashSet;

use bstr::BString;

use crate::EnvVarsState;

pub fn export(
    env_vars_state: EnvVarsState,
    _semicolon_delimited_env_vars: Option<&HashSet<String>>,
) -> BString {
    serde_json::to_string_pretty(&env_vars_state)
        .unwrap_or_else(|_| String::from("{}"))
        .into()
}
