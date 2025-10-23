mod config;
mod opt;

use std::{
    collections::{BTreeMap, HashSet},
    env::current_exe,
    io::Read,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
};

use base64::{Engine, prelude::BASE64_STANDARD};
use bstr::B;
use clap::Parser;
use env_hooks::{
    BashSource, EnvVars, EnvVarsState, env_vars_state_from_env_vars, get_env_vars_from_bash,
    get_env_vars_from_current_process, get_env_vars_reset, get_old_env_vars_to_be_updated,
    merge_delimited_env_var, shells,
    state::{self, GetEnvStateVar, MatchRcs, ReadyForFullResetOrDone},
};
use nix_dev_env::{NixProfileCache, check_nix_version};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shell_quote::Bash;

use crate::{
    config::{EnvoluntaryConfig, get_cache_dir, get_config_path},
    opt::{
        Envoluntary, EnvoluntaryCommands, EnvoluntaryConfigCommands, EnvoluntaryShell,
        EnvoluntaryShellCommands,
    },
};

const ENVOLUNTARY_ENV_STATE_VAR_KEY: &str = "ENVOLUNTARY_ENV_STATE";

const ENV_VAR_KEY_PATH: &str = "PATH";
const ENV_VAR_KEY_XDG_DATA_DIRS: &str = "XDG_DATA_DIRS";

static SEMICOLON_DELIMITED_ENV_VARS: Lazy<HashSet<String>> = Lazy::new(|| {
    let mut semicolon_delimited_env_vars = HashSet::new();
    semicolon_delimited_env_vars.insert(String::from(ENV_VAR_KEY_PATH));
    semicolon_delimited_env_vars.insert(String::from(ENV_VAR_KEY_XDG_DATA_DIRS));
    semicolon_delimited_env_vars
});

fn main() -> anyhow::Result<()> {
    let opt = Envoluntary::parse();

    if let Some(command) = opt.command {
        match command {
            EnvoluntaryCommands::Config { config } => match config {
                EnvoluntaryConfigCommands::PrintPath => {
                    config::print_path()?;
                }
                EnvoluntaryConfigCommands::Edit {
                    config_path,
                    editor_program,
                } => config::edit(config_path.as_deref(), editor_program.as_deref())?,
                EnvoluntaryConfigCommands::AddEntry {
                    config_path,
                    pattern,
                    flake_reference,
                } => {
                    config::add_entry(config_path.as_deref(), pattern, flake_reference)?;
                }
                EnvoluntaryConfigCommands::PrintMatchingEntries { config_path, path } => {
                    config::print_matching_entries(config_path.as_deref(), &path)?;
                }
            },
            // TODO: Clean this up into its own module
            EnvoluntaryCommands::Shell { shell } => match shell {
                EnvoluntaryShellCommands::Hook { shell } => match shell {
                    EnvoluntaryShell::Fish => {
                        println!(
                            "{}",
                            shells::fish::hook(
                                "envoluntary",
                                bstr::join(
                                    " ",
                                    [&Bash::quote_vec(&current_exe()?), B("export fish")]
                                )
                            )
                        );
                    }
                },
                EnvoluntaryShellCommands::Export {
                    config_path,
                    shell: _,
                    cache_dir,
                    flake_references,
                    force_update,
                    current_dir,
                } => {
                    // TODO: Use `shell`
                    let config_path = get_config_path(config_path.as_deref())?;
                    let envoluntary_config = EnvoluntaryConfig::load(&config_path)?;
                    let cache_dir = get_cache_dir(cache_dir.as_deref())?;

                    check_nix_version()?;

                    let current_dir_state = state::ShellPromptState::get_current_dir(current_dir)?;

                    let match_rcs = current_dir_state.match_rcs(|current_dir| {
                        let flake_references = if let Some(ref flake_references) = flake_references
                        {
                            flake_references.clone()
                        } else {
                            envoluntary_config
                                .matching_entries(current_dir)
                                .into_iter()
                                .map(|entry| entry.flake_reference)
                                .collect()
                        };
                        Ok(flake_references)
                    })?;

                    match match_rcs {
                        MatchRcs::NoRcs(no_rcs_state) => {
                            let ready_for_full_reset_or_done =
                                no_rcs_state.get_env_state_var(ENVOLUNTARY_ENV_STATE_VAR_KEY);
                            match ready_for_full_reset_or_done {
                                ReadyForFullResetOrDone::ReadyForFullReset(
                                    ready_for_full_reset_state,
                                ) => ready_for_full_reset_state.reset_env_vars(
                                    |env_state_var_value| {
                                        let env_state = EnvoluntaryEnvState::decode(
                                            env_state_var_value.as_bytes(),
                                        )?;
                                        println!(
                                            "{}",
                                            shells::fish::export(
                                                env_state.env_vars_reset,
                                                Some(&SEMICOLON_DELIMITED_ENV_VARS)
                                            )
                                        );
                                        Ok(())
                                    },
                                )?,
                                ReadyForFullResetOrDone::Done => {
                                    // 🤫 nothing to do
                                }
                            };
                        }
                        MatchRcs::Rcs(rcs_state) => {
                            let get_env_state_var =
                                rcs_state.get_env_state_var(ENVOLUNTARY_ENV_STATE_VAR_KEY);
                            match get_env_state_var {
                                GetEnvStateVar::NoEnvStateVar(no_env_state_var_state) => {
                                    no_env_state_var_state.set_new_env_state_var(|rcs| {
                                        // TODO: Make this work for more than 1 flake matching
                                        for flake_reference in rcs {
                                            let cache_profile = get_cache_profile(
                                                cache_dir.clone(),
                                                flake_reference.clone(),
                                                force_update,
                                            )?;
                                            export_env_vars(
                                                flake_reference.clone(),
                                                &cache_profile,
                                            )?;
                                        }
                                        Ok(())
                                    })?;
                                }
                                GetEnvStateVar::EnvStateVar(env_state_var_state) => {
                                    env_state_var_state.reset_and_set_new_env_state_var(
                                        |rcs, env_state_var_value| {
                                            let env_state = EnvoluntaryEnvState::decode(
                                                env_state_var_value.as_bytes(),
                                            )?;

                                            if rcs == env_state.flake_references {
                                                return Ok((rcs, env_state.flake_references));
                                            }

                                            println!(
                                                "{}",
                                                shells::fish::export(
                                                    env_state.env_vars_reset,
                                                    Some(&SEMICOLON_DELIMITED_ENV_VARS)
                                                )
                                            );
                                            Ok((rcs, env_state.flake_references))
                                        },
                                        |(rcs, env_state_flake_references)| {
                                            if rcs == env_state_flake_references {
                                                return Ok(());
                                            }

                                            // TODO: Make this work for more than 1 flake matching
                                            for flake_reference in rcs {
                                                let cache_profile = get_cache_profile(
                                                    cache_dir.clone(),
                                                    flake_reference.clone(),
                                                    force_update,
                                                )?;
                                                export_env_vars(
                                                    flake_reference.clone(),
                                                    &cache_profile,
                                                )?;
                                            }
                                            Ok(())
                                        },
                                    )?;
                                }
                            };
                        }
                    };
                }
            },
        }
    };

    Ok(())
}

fn get_cache_profile(
    cache_dir: PathBuf,
    flake_reference: String,
    force_update: bool,
) -> anyhow::Result<NixProfileCache> {
    let cache_profile = NixProfileCache::new(cache_dir, flake_reference)?;

    if force_update || cache_profile.needs_update()? {
        cache_profile.update()?;
    }

    Ok(cache_profile)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnvoluntaryEnvState {
    pub flake_references: Vec<String>,
    pub env_vars_reset: EnvVarsState,
}

impl EnvoluntaryEnvState {
    pub fn decode(base64_value: impl AsRef<[u8]>) -> anyhow::Result<Self> {
        let zstd_value = BASE64_STANDARD.decode(base64_value)?;
        let mut zstd_value_slice = zstd_value.as_slice();
        let mut zstd_decoder = ruzstd::decoding::StreamingDecoder::new(&mut zstd_value_slice)?;
        let mut value = vec![];
        zstd_decoder.read_to_end(&mut value)?;
        Ok(serde_json::from_slice(&value)?)
    }

    pub fn encode(&self) -> anyhow::Result<String> {
        let value = serde_json::to_vec(self)?;
        let value_slice = value.as_slice();
        let zstd_value = ruzstd::encoding::compress_to_vec(
            value_slice,
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        Ok(BASE64_STANDARD.encode(zstd_value))
    }
}

fn export_env_vars(flake_reference: String, cache_profile: &NixProfileCache) -> anyhow::Result<()> {
    let EnvVarUpdates {
        mut new_env_vars,
        old_env_vars_to_be_updated,
    } = get_new_env_vars(cache_profile)?;
    let env_vars_reset = get_env_vars_reset(
        old_env_vars_to_be_updated,
        new_env_vars.keys().cloned().collect(),
        String::from(ENVOLUNTARY_ENV_STATE_VAR_KEY),
    );
    let env_state = EnvoluntaryEnvState {
        flake_references: vec![flake_reference],
        env_vars_reset,
    };
    new_env_vars.insert(
        String::from(ENVOLUNTARY_ENV_STATE_VAR_KEY),
        env_state.encode()?,
    );
    println!(
        "{}",
        shells::fish::export(
            env_vars_state_from_env_vars(new_env_vars),
            Some(&SEMICOLON_DELIMITED_ENV_VARS)
        )
    );
    Ok(())
}

struct EnvVarUpdates {
    new_env_vars: EnvVars,
    old_env_vars_to_be_updated: EnvVars,
}

fn get_new_env_vars(cache_profile: &NixProfileCache) -> anyhow::Result<EnvVarUpdates> {
    let mut bash_env_vars = BTreeMap::new();
    // Prints devshell "message of the day" the same way it would in `direnv`
    // https://github.com/numtide/devshell/blob/7c9e793ebe66bcba8292989a68c0419b737a22a0/modules/devshell.nix#L400
    bash_env_vars.insert(String::from("DIRENV_IN_ENVRC"), String::from("1"));

    let mut new_env_vars = get_env_vars_from_bash(
        BashSource::File(PathBuf::from(cache_profile.profile_rc())),
        Some(bash_env_vars),
    )?;

    let old_env_vars_to_be_updated = {
        let old_env_vars = get_env_vars_from_current_process();
        get_old_env_vars_to_be_updated(old_env_vars, &new_env_vars)
    };

    merge_delimited_env_var(
        ENV_VAR_KEY_PATH,
        ':',
        ':',
        &old_env_vars_to_be_updated,
        &mut new_env_vars,
    );
    merge_delimited_env_var(
        ENV_VAR_KEY_XDG_DATA_DIRS,
        ':',
        ':',
        &old_env_vars_to_be_updated,
        &mut new_env_vars,
    );

    Ok(EnvVarUpdates {
        new_env_vars,
        old_env_vars_to_be_updated,
    })
}
