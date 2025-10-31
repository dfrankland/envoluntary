pub mod shells;
pub mod state;

use std::{
    collections::HashSet,
    env, fs, num,
    ops::{Deref, DerefMut},
    path::PathBuf,
    process::ExitStatus,
};

use bstr::{B, BString, ByteSlice};
use duct::cmd;
use indexmap::{IndexMap, IndexSet, map::IntoIter};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shell_quote::Bash;

type EnvVarsInner = IndexMap<String, String>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvVars(EnvVarsInner);

impl EnvVars {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Deref for EnvVars {
    type Target = EnvVarsInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EnvVars {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for EnvVars {
    type Item = (String, String);
    type IntoIter = EnvVarIntoIter<String, String>;

    fn into_iter(self) -> Self::IntoIter {
        EnvVarIntoIter(self.0.into_iter())
    }
}

impl FromIterator<(String, String)> for EnvVars {
    fn from_iter<I: IntoIterator<Item = (String, String)>>(iter: I) -> Self {
        EnvVars(EnvVarsInner::from_iter(iter))
    }
}

impl From<EnvVars> for EnvVarsState {
    fn from(value: EnvVars) -> Self {
        EnvVarsState(value.0.into_iter().map(|(k, v)| (k, Some(v))).collect())
    }
}

type EnvVarsStateInner = IndexMap<String, Option<String>>;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvVarsState(EnvVarsStateInner);

impl EnvVarsState {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Deref for EnvVarsState {
    type Target = EnvVarsStateInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EnvVarsState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for EnvVarsState {
    type Item = (String, Option<String>);
    type IntoIter = EnvVarIntoIter<String, Option<String>>;

    fn into_iter(self) -> Self::IntoIter {
        EnvVarIntoIter(self.0.into_iter())
    }
}

impl FromIterator<(String, Option<String>)> for EnvVarsState {
    fn from_iter<I: IntoIterator<Item = (String, Option<String>)>>(iter: I) -> Self {
        EnvVarsState(EnvVarsStateInner::from_iter(iter))
    }
}

type EnvVarInner<K, V> = IntoIter<K, V>;

pub struct EnvVarIntoIter<K, V>(EnvVarInner<K, V>);

impl<K, V> Deref for EnvVarIntoIter<K, V> {
    type Target = EnvVarInner<K, V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V> DerefMut for EnvVarIntoIter<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K, V> Iterator for EnvVarIntoIter<K, V> {
    type Item = (K, V);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

pub fn get_old_env_vars_to_be_updated(old_env_vars: EnvVars, new_env_vars: &EnvVars) -> EnvVars {
    old_env_vars
        .into_iter()
        .fold(EnvVars::new(), |mut acc, (key, value)| {
            if new_env_vars.contains_key(&key) && new_env_vars.get(&key) != Some(&value) {
                acc.insert(key, value);
            }
            acc
        })
}

pub fn get_env_vars_reset(
    mut old_env_vars_that_were_updated: EnvVars,
    new_env_vars: HashSet<String>,
    env_state_var_key: String,
) -> EnvVarsState {
    let mut env_vars_state = new_env_vars
        .into_iter()
        .fold(EnvVarsState::new(), |mut acc, key| {
            let value = old_env_vars_that_were_updated.shift_remove(&key);
            acc.insert(key, value);
            acc
        });
    env_vars_state.insert(env_state_var_key, None);
    env_vars_state
}

pub fn get_env_vars_from_current_process() -> EnvVars {
    EnvVars(env::vars().collect::<EnvVarsInner>())
}

pub enum BashSource {
    File(PathBuf),
    Script(BString),
}

impl AsRef<BashSource> for BashSource {
    fn as_ref(&self) -> &BashSource {
        self
    }
}

impl BashSource {
    fn to_command_string(&self) -> BString {
        match &self {
            Self::File(path) => bstr::join(" ", [B("source"), &Bash::quote_vec(path)]).into(),
            Self::Script(script) => bstr::join(" ", [B("eval"), &Bash::quote_vec(script)]).into(),
        }
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

    let command_string = bstr::join(
        " ",
        [
            &source.as_ref().to_command_string(),
            B("&& env -0 >"),
            &Bash::quote_vec(bash_env_vars_file.path()),
        ],
    );
    let handle = cmd!("bash", "-c", command_string.to_os_str()?)
        .full_env(env_vars.unwrap_or_default())
        .stdout_to_stderr()
        .start()?;
    let output = handle.wait()?;
    output
        .status
        .simplified_exit_ok()
        .map_err(|e| anyhow::format_err!("Bash command to retrieve env vars failed:\n{e}"))?;

    let bash_env_vars_string = fs::read_to_string(bash_env_vars_file.path())?;

    let bash_env_vars = EnvVars(
        bash_env_vars_string
            .split('\0')
            .filter_map(|env_var| env_var.split_once('='))
            .map(|(key, value)| (String::from(key), String::from(value)))
            .collect::<EnvVarsInner>(),
    );

    Ok(bash_env_vars)
}

pub fn merge_delimited_env_var(
    env_var: &str,
    split_delimiter: char,
    join_delimiter: char,
    old_env_vars: &EnvVars,
    new_env_vars: &mut EnvVars,
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

const IGNORED_ENV_VAR_PREFIXES: &[&str] = &["__fish", "BASH_FUNC_"];

static IGNORED_ENV_VAR_KEYS: Lazy<HashSet<&str>> = Lazy::new(|| {
    HashSet::from([
        // direnv env config
        "DIRENV_CONFIG",
        "DIRENV_BASH",
        // should only be available inside of the .envrc or .env
        "DIRENV_IN_ENVRC",
        "COMP_WORDBREAKS", // Avoids segfaults in bash
        "PS1",             // PS1 should not be exported, fixes problem in bash
        // variables that should change freely
        "OLDPWD",
        "PWD",
        "SHELL",
        "SHELLOPTS",
        "SHLVL",
        "_",
    ])
});

pub fn ignored_env_var_key(env_var_key: &str) -> bool {
    for ignored_env_var_prefix in IGNORED_ENV_VAR_PREFIXES {
        if env_var_key.starts_with(ignored_env_var_prefix) {
            return true;
        }
    }
    IGNORED_ENV_VAR_KEYS.contains(env_var_key)
}

pub fn remove_ignored_env_vars(env_vars: &mut EnvVars) {
    let env_var_keys = env_vars.keys().cloned().collect::<Vec<_>>();
    env_var_keys.into_iter().for_each(|env_var_key| {
        if ignored_env_var_key(&env_var_key) {
            env_vars.shift_remove(&env_var_key);
        }
    });
}
