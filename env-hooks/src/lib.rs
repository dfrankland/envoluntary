pub mod shells;

use std::{
    collections::{BTreeMap, HashSet},
    env,
    ffi::{OsStr, OsString},
    fs, num,
    path::PathBuf,
    process::{Command, ExitStatus},
};

use indexmap::IndexSet;
use shell_quote::{Bash, QuoteExt};

pub type EnvVars = BTreeMap<String, String>;
pub type EnvVarsState = BTreeMap<String, Option<String>>;

pub fn get_old_env_vars_to_be_updated(old_env_vars: EnvVars, new_env_vars: &EnvVars) -> EnvVars {
    old_env_vars
        .into_iter()
        .fold(BTreeMap::new(), |mut acc, (key, value)| {
            if new_env_vars.contains_key(&key) && new_env_vars.get(&key) != Some(&value) {
                acc.insert(key, value);
            }
            acc
        })
}

// TODO: Create new type patern for `EnvVars` and `EnvVarsState`?
// impl From<EnvVars> for EnvVarsState {
//   fn from(value: EnvVars) -> Self {
//     value
//       .into_iter()
//       .map(|(key, value)| (key, Some(value)))
//       .collect()
//   }
// }
pub fn env_vars_state_from_env_vars(env_vars: EnvVars) -> EnvVarsState {
    env_vars
        .into_iter()
        .map(|(key, value)| (key, Some(value)))
        .collect()
}

pub fn get_env_vars_reset(
    mut old_env_vars_that_were_updated: EnvVars,
    new_env_vars: HashSet<String>,
) -> EnvVarsState {
    new_env_vars
        .into_iter()
        .fold(EnvVarsState::new(), |mut acc, key| {
            let value = old_env_vars_that_were_updated.remove(&key);
            acc.insert(key, value);
            acc
        })
}

pub fn get_env_vars_from_current_process() -> EnvVars {
    env::vars().collect::<EnvVars>()
}

pub enum BashSource {
    File(PathBuf),
    Script(OsString),
}

impl AsRef<BashSource> for BashSource {
    fn as_ref(&self) -> &BashSource {
        self
    }
}

impl BashSource {
    fn to_command_string(&self) -> OsString {
        let mut command_string = OsString::new();
        match &self {
            Self::File(path) => {
                command_string.push("source ");
                command_string.push_quoted(Bash, path.as_os_str());
            }
            Self::Script(script) => {
                command_string.push("eval ");
                command_string.push_quoted(Bash, script);
            }
        };
        command_string
    }
}

pub(crate) trait SimplifiedExitOk {
    fn simplified_exit_ok(&self) -> anyhow::Result<()>;
}

impl SimplifiedExitOk for ExitStatus {
    /// Simplified implementation of <https://github.com/rust-lang/rust/issues/84908>
    // TODO: Remove this and use `exit_ok` when it's stabilized.
    fn simplified_exit_ok(&self) -> anyhow::Result<()> {
        match num::NonZero::try_from(self.code().unwrap_or(-1)) {
            Ok(_) => Err(anyhow::format_err!(
                "process exited unsuccessfully: {}",
                &self
            )),
            Err(_) => Ok(()),
        }
    }
}

pub fn get_env_vars_from_bash(
    source: impl AsRef<BashSource>,
    env_vars: Option<EnvVars>,
) -> anyhow::Result<EnvVars> {
    let bash_env_vars_file = tempfile::NamedTempFile::new()?;

    let mut command_string = source.as_ref().to_command_string();
    command_string.push(" && env -0 > ");
    command_string.push_quoted(Bash, bash_env_vars_file.path().as_os_str());

    let mut command = Command::new("bash");
    command
        .args([OsStr::new("-c"), &command_string])
        .env_clear();

    if let Some(env_vars) = env_vars {
        command.envs(env_vars);
    }

    let exit_status = command.spawn()?.wait()?;
    exit_status
        .simplified_exit_ok()
        .map_err(|e| anyhow::format_err!("Bash command to retrieve env vars failed:\n{e}"))?;

    let bash_env_vars_string = fs::read_to_string(bash_env_vars_file.path())?;

    let bash_env_vars = bash_env_vars_string
        .split('\0')
        .filter_map(|env_var| env_var.split_once('='))
        .map(|(key, value)| (String::from(key), String::from(value)))
        .collect::<EnvVars>();

    Ok(bash_env_vars)
}

pub fn merge_delimited_env_var(
    env_var: &str,
    split_delimiter: char,
    join_delimiter: char,
    old_env_vars: &BTreeMap<String, String>,
    new_env_vars: &mut BTreeMap<String, String>,
) {
    if let (Some(old_value), Some(new_value)) =
        (old_env_vars.get(env_var), new_env_vars.get_mut(env_var))
    {
        *new_value = merge_delimited_values(split_delimiter, join_delimiter, old_value, new_value);
    }
}

pub fn merge_delimited_values(
    split_delimiter: char,
    join_delimiter: char,
    old_value: &str,
    new_value: &str,
) -> String {
    new_value
        .split(split_delimiter)
        .chain(old_value.split(split_delimiter))
        .collect::<IndexSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(&join_delimiter.to_string())
}
