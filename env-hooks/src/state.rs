// TODO: Document state flow with `aquamarine` crate
// flowchart TD
//     subgraph getEnvStateVarFn
//         getEnvStateVar1
//         getEnvStateVar2
//     end
//     subgraph getNewEnvStateVarFn
//         getNewEnvStateVar1
//         getNewEnvStateVar2
//     end
//     subgraph resetEnvVarsFn
//         resetEnvVars1
//         resetEnvVars2
//     end
//     ShellPromptState --> getCurrentDir@{ shape: "diamond" }
//     getCurrentDir --> |success| CurrentDirState
//     CurrentDirState --> findAllMatchingRcs@{ shape: "diamond" }
//     findAllMatchingRcs --> |no rcs matched| NoRcsState
//     findAllMatchingRcs --> |rcs matched| RcsState
//     NoRcsState --> getEnvStateVar1@{ shape: "diamond", label: "getEnvStateVar" }
//     RcsState --> getEnvStateVar2@{ shape: "diamond", label: "getEnvStateVar" }
//     getEnvStateVar1 --> |no env state var found| DoneState
//     getEnvStateVar1 --> |env state var found| ReadyForFullResetState
//     getEnvStateVar2 --> |no env state var found| NoEnvStateVarState
//     getEnvStateVar2 --> |env state var found| EnvStateVarState
//     ReadyForFullResetState --> resetEnvVars1@{ shape: "diamond", label: "resetEnvVars" }
//     resetEnvVars1 --> |success| DoneState
//     NoEnvStateVarState --> getNewEnvStateVar1@{ shape: "diamond", label: "getNewEnvStateVar" }
//     EnvStateVarState --> getNewEnvStateVar2@{ shape: "diamond", label: "getNewEnvStateVar" }
//     getNewEnvStateVar1 --> |success| NewEnvStateVarState
//     getNewEnvStateVar2 --> |success| OldAndNewEnvStateVarState
//     NewEnvStateVarState --> setEnvVars@{ shape: "diamond" }
//     OldAndNewEnvStateVarState --> resetEnvVars2@{ shape: "diamond", label: "resetEnvVars" }
//     resetEnvVars2 --> |success| setEnvVars
//     setEnvVars --> |success| DoneState

use std::result::Result;
use std::{
    env,
    ffi::{OsStr, OsString},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use anyhow::Ok;

#[derive(Debug, Clone, Copy, Default)]
pub struct ShellPromptState;

impl ShellPromptState {
    pub fn get_current_dir(
        provided_current_dir: Option<PathBuf>,
    ) -> anyhow::Result<CurrentDirState> {
        let current_dir = if let Some(current_dir) = provided_current_dir {
            current_dir
        } else {
            env::current_dir()?
        };
        Ok(CurrentDirState { current_dir })
    }
    pub fn check_files_backwards<T: Fn(&str) -> bool>(dir: &Path, check_cb: T) -> bool {
        if let Result::Ok(entries) = dir.read_dir() {
            for entry in entries {
                if let Result::Ok(entry) = entry
                    && check_cb(&entry.file_name().to_string_lossy())
                {
                    return true;
                }
            }
        }
        if let Some(parent) = dir.parent() {
            Self::check_files_backwards(parent, check_cb)
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct CurrentDirState {
    current_dir: PathBuf,
}

impl CurrentDirState {
    pub fn match_rcs<RC>(
        self,
        match_rcs_cb: impl Fn(&Path) -> anyhow::Result<Vec<RC>>,
    ) -> anyhow::Result<MatchRcs<RC>> {
        let rcs = match_rcs_cb(&self.current_dir)?;
        let match_rcs = if rcs.is_empty() {
            MatchRcs::NoRcs(NoRcsState {
                phantom: PhantomData,
            })
        } else {
            MatchRcs::Rcs(RcsState { rcs })
        };
        Ok(match_rcs)
    }
}

#[derive(Debug, Clone)]
pub enum MatchRcs<RC> {
    NoRcs(NoRcsState),
    Rcs(RcsState<RC>),
}

#[derive(Debug, Clone)]
pub struct NoRcsState {
    phantom: PhantomData<()>,
}

impl NoRcsState {
    pub fn get_env_state_var(
        self,
        env_state_var_key: impl AsRef<OsStr>,
    ) -> Option<ReadyForFullResetState> {
        get_env_state_var(env_state_var_key).map(|env_state_var_value| ReadyForFullResetState {
            env_state_var_value,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ReadyForFullResetState {
    env_state_var_value: OsString,
}

impl ReadyForFullResetState {
    pub fn reset_env_vars(
        self,
        reset_env_vars_cb: impl Fn(OsString) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        reset_env_vars_cb(self.env_state_var_value)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct RcsState<RC> {
    rcs: Vec<RC>,
}

impl<RC> RcsState<RC> {
    pub fn get_env_state_var(self, env_state_var_key: impl AsRef<OsStr>) -> GetEnvStateVar<RC> {
        let rcs = self.rcs;
        if let Some(env_state_var_value) = get_env_state_var(env_state_var_key) {
            GetEnvStateVar::EnvStateVar(EnvStateVarState {
                rcs,
                env_state_var_value,
            })
        } else {
            GetEnvStateVar::NoEnvStateVar(NoEnvStateVarState { rcs })
        }
    }
}

#[derive(Debug, Clone)]
pub enum GetEnvStateVar<RC> {
    NoEnvStateVar(NoEnvStateVarState<RC>),
    EnvStateVar(EnvStateVarState<RC>),
}

#[derive(Debug, Clone)]
pub struct NoEnvStateVarState<RC> {
    rcs: Vec<RC>,
}

impl<RC> NoEnvStateVarState<RC> {
    pub fn set_new_env_state_var(
        self,
        set_new_env_state_var_cb: impl Fn(Vec<RC>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        set_new_env_state_var_cb(self.rcs)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EnvStateVarState<RC> {
    rcs: Vec<RC>,
    env_state_var_value: OsString,
}

impl<RC> EnvStateVarState<RC> {
    pub fn reset_and_set_new_env_state_var<T>(
        self,
        reset_env_vars_cb: impl Fn(Vec<RC>, OsString) -> anyhow::Result<T>,
        set_new_env_state_var_cb: impl Fn(T) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        let state = reset_env_vars_cb(self.rcs, self.env_state_var_value)?;
        set_new_env_state_var_cb(state)?;
        Ok(())
    }
}

fn get_env_state_var(env_state_var_key: impl AsRef<OsStr>) -> Option<OsString> {
    env::var_os(env_state_var_key)
}
