use std::collections::HashSet;
use std::env;
use std::path::Path;
use std::{collections::BTreeMap, io::Read, os::unix::ffi::OsStrExt, path::PathBuf};

use base64::{Engine, prelude::BASE64_STANDARD};
use bstr::B;
use env_hooks::{
    BashSource, EnvVars, EnvVarsState, env_vars_state_from_env_vars, get_env_vars_from_bash,
    get_env_vars_from_current_process, get_env_vars_reset, get_old_env_vars_to_be_updated,
    merge_delimited_env_var, shells,
    state::{self, GetEnvStateVar, MatchRcs},
};
use nix_dev_env::{NixProfileCache, check_nix_version};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use shell_quote::Fish;

use crate::config::{EnvoluntaryConfig, get_cache_dir, get_config_path};
use crate::opt::{
    EnvoluntaryShell, EnvoluntaryShellExportArgs, EnvoluntaryShellPrintCachePathArgs,
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

pub fn print_hook(shell: EnvoluntaryShell) -> anyhow::Result<()> {
    match shell {
        EnvoluntaryShell::Fish => {
            println!(
                "{}",
                shells::fish::hook(
                    "envoluntary",
                    bstr::join(
                        " ",
                        [&Fish::quote_vec(&env::current_exe()?), B("export fish")]
                    )
                )
            );
        }
    }

    Ok(())
}

pub fn print_export(args: EnvoluntaryShellExportArgs) -> anyhow::Result<()> {
    let config_path = get_config_path(args.config_path.as_deref())?;
    let envoluntary_config = EnvoluntaryConfig::load(&config_path)?;
    let cache_dir = get_cache_dir(args.cache_dir.as_deref())?;

    check_nix_version()?;

    let current_dir_state = state::ShellPromptState::get_current_dir(args.current_dir)?;

    let match_rcs = current_dir_state.match_rcs(|current_dir| {
        let flake_references = if let Some(ref flake_references) = args.flake_references {
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
            if let Some(ready_for_full_reset_state) =
                no_rcs_state.get_env_state_var(ENVOLUNTARY_ENV_STATE_VAR_KEY)
            {
                ready_for_full_reset_state.reset_env_vars(|env_state_var_value| {
                    let env_state = EnvoluntaryEnvState::decode(env_state_var_value.as_bytes())?;
                    print_shell_export(args.shell, env_state.env_vars_reset);
                    Ok(())
                })?;
            }
        }
        MatchRcs::Rcs(rcs_state) => {
            let get_env_state_var = rcs_state.get_env_state_var(ENVOLUNTARY_ENV_STATE_VAR_KEY);
            match get_env_state_var {
                GetEnvStateVar::NoEnvStateVar(no_env_state_var_state) => {
                    no_env_state_var_state.set_new_env_state_var(|rcs| {
                        let env_vars_state = rcs.into_iter().try_fold(
                            EnvVarsState::new(),
                            |mut acc, flake_reference| -> anyhow::Result<EnvVarsState> {
                                let cache_profile = get_cache_profile(
                                    &cache_dir,
                                    &flake_reference,
                                    args.force_update,
                                )?;
                                acc.extend(get_export_env_vars_state(
                                    flake_reference,
                                    &cache_profile,
                                )?);
                                Ok(acc)
                            },
                        )?;

                        print_shell_export(args.shell, env_vars_state);

                        Ok(())
                    })?;
                }
                GetEnvStateVar::EnvStateVar(env_state_var_state) => {
                    env_state_var_state.reset_and_set_new_env_state_var(
                        |rcs, env_state_var_value| {
                            let env_state =
                                EnvoluntaryEnvState::decode(env_state_var_value.as_bytes())?;

                            if rcs == env_state.flake_references {
                                return Ok((rcs, env_state.flake_references));
                            }

                            print_shell_export(args.shell, env_state.env_vars_reset);

                            Ok((rcs, env_state.flake_references))
                        },
                        |(rcs, env_state_flake_references)| {
                            if rcs == env_state_flake_references {
                                return Ok(());
                            }

                            let env_vars_state = rcs.into_iter().try_fold(
                                EnvVarsState::new(),
                                |mut acc, flake_reference| -> anyhow::Result<EnvVarsState> {
                                    let cache_profile = get_cache_profile(
                                        &cache_dir,
                                        &flake_reference,
                                        args.force_update,
                                    )?;
                                    acc.extend(get_export_env_vars_state(
                                        flake_reference,
                                        &cache_profile,
                                    )?);
                                    Ok(acc)
                                },
                            )?;

                            print_shell_export(args.shell, env_vars_state);

                            Ok(())
                        },
                    )?;
                }
            };
        }
    };

    Ok(())
}

pub fn print_cache_path(args: EnvoluntaryShellPrintCachePathArgs) -> anyhow::Result<()> {
    let cache_dir = get_cache_dir(args.cache_dir.as_deref())?;
    println!(
        "{}",
        get_cache_sub_dir(&cache_dir, &args.flake_reference).display()
    );
    Ok(())
}

fn get_cache_profile(
    cache_dir: &Path,
    flake_reference: &str,
    force_update: bool,
) -> anyhow::Result<NixProfileCache> {
    let cach_sub_dir = get_cache_sub_dir(cache_dir, flake_reference);
    let cache_profile = NixProfileCache::new(cach_sub_dir, flake_reference)?;

    if force_update || cache_profile.needs_update()? {
        cache_profile.update()?;
    }

    Ok(cache_profile)
}

fn get_cache_sub_dir(cache_dir: &Path, flake_reference: &str) -> PathBuf {
    cache_dir.join(format!("{:x}", Sha1::digest(flake_reference)))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EnvoluntaryEnvState {
    flake_references: Vec<String>,
    env_vars_reset: EnvVarsState,
}

impl EnvoluntaryEnvState {
    fn decode(base64_value: impl AsRef<[u8]>) -> anyhow::Result<Self> {
        let zstd_value = BASE64_STANDARD.decode(base64_value)?;
        let mut zstd_value_slice = zstd_value.as_slice();
        let mut zstd_decoder = ruzstd::decoding::StreamingDecoder::new(&mut zstd_value_slice)?;
        let mut value = vec![];
        zstd_decoder.read_to_end(&mut value)?;
        Ok(serde_json::from_slice(&value)?)
    }

    fn encode(&self) -> anyhow::Result<String> {
        let value = serde_json::to_vec(self)?;
        let value_slice = value.as_slice();
        let zstd_value = ruzstd::encoding::compress_to_vec(
            value_slice,
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        Ok(BASE64_STANDARD.encode(zstd_value))
    }
}

fn get_export_env_vars_state(
    flake_reference: String,
    cache_profile: &NixProfileCache,
) -> anyhow::Result<EnvVarsState> {
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
    Ok(env_vars_state_from_env_vars(new_env_vars))
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

fn print_shell_export(shell: EnvoluntaryShell, env_vars_state: EnvVarsState) {
    let export = match shell {
        EnvoluntaryShell::Fish => {
            shells::fish::export(env_vars_state, Some(&SEMICOLON_DELIMITED_ENV_VARS))
        }
    };
    println!("{}", export);
}
