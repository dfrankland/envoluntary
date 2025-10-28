use std::{
    collections::HashSet,
    env, fs,
    io::{Read, Write},
    path::PathBuf,
};

use base64::{Engine, prelude::BASE64_URL_SAFE_NO_PAD};
use bstr::B;
use env_hooks::{
    BashSource, EnvVars, EnvVarsState, env_vars_state_from_env_vars, get_env_vars_from_bash,
    get_env_vars_from_current_process, get_env_vars_reset, get_old_env_vars_to_be_updated,
    merge_delimited_env_var, remove_ignored_env_vars, shells,
    state::{self, GetEnvStateVar, MatchRcs},
};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use shell_quote::{Bash, Fish, Zsh};

use crate::opt::{DirenvShell, DirenvShellExportArgs};

const CLI_NAME: &str = "direnv";

const DIRENV_ENV_STATE_VAR_KEY: &str = "DIRENV_DIFF";
const DIRENV_FILE_VAR_KEY: &str = "DIRENV_FILE";

const ENV_VAR_KEY_PATH: &str = "PATH";
const ENV_VAR_KEY_XDG_DATA_DIRS: &str = "XDG_DATA_DIRS";

static SEMICOLON_DELIMITED_ENV_VARS: Lazy<HashSet<String>> = Lazy::new(|| {
    let mut semicolon_delimited_env_vars = HashSet::new();
    semicolon_delimited_env_vars.insert(String::from(ENV_VAR_KEY_PATH));
    semicolon_delimited_env_vars.insert(String::from(ENV_VAR_KEY_XDG_DATA_DIRS));
    semicolon_delimited_env_vars
});

pub fn print_hook(shell: DirenvShell) -> anyhow::Result<()> {
    let current_exe = env::current_exe()?;

    let hook = match shell {
        DirenvShell::Bash => shells::bash::hook(
            CLI_NAME,
            bstr::join(" ", [&Bash::quote_vec(&current_exe), B("export bash")]),
        ),
        DirenvShell::Fish => shells::fish::hook(
            CLI_NAME,
            bstr::join(" ", [&Fish::quote_vec(&current_exe), B("export fish")]),
        ),
        DirenvShell::Json => {
            return Err(anyhow::anyhow!(
                "JSON isn't is a shell, so there's no hook to use."
            ));
        }
        DirenvShell::Zsh => shells::zsh::hook(
            CLI_NAME,
            bstr::join(" ", [&Zsh::quote_vec(&current_exe), B("export zsh")]),
        ),
    };

    println!("{}", hook);

    Ok(())
}

pub fn print_export(args: DirenvShellExportArgs) -> anyhow::Result<()> {
    let current_dir_state = state::ShellPromptState::get_current_dir(None)?;

    let match_rcs = current_dir_state.match_rcs(|current_dir| {
        let rcs = find_envrc_walking_up_file_hierarchy(PathBuf::from(current_dir))
            .into_iter()
            .collect::<Vec<_>>();
        Ok(rcs)
    })?;

    match match_rcs {
        MatchRcs::NoRcs(no_rcs_state) => {
            if let Some(ready_for_full_reset_state) =
                no_rcs_state.get_env_state_var(DIRENV_ENV_STATE_VAR_KEY)
            {
                ready_for_full_reset_state.reset_env_vars(|env_state_var_value| {
                    let direnv_diff = DirenvDiff::decode(&env_state_var_value.to_string_lossy())?;
                    print_shell_export(args.shell, direnv_diff.get_env_vars_reset());
                    Ok(())
                })?;
            }
        }
        MatchRcs::Rcs(rcs_state) => {
            let get_env_state_var = rcs_state.get_env_state_var(DIRENV_ENV_STATE_VAR_KEY);
            match get_env_state_var {
                GetEnvStateVar::NoEnvStateVar(no_env_state_var_state) => {
                    no_env_state_var_state.set_new_env_state_var(|rcs| {
                        let env_vars_state = rcs.into_iter().try_fold(
                            EnvVarsState::new(),
                            |mut acc, envrc| -> anyhow::Result<EnvVarsState> {
                                acc.extend(get_export_env_vars_state(envrc)?);
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
                            let direnv_diff =
                                DirenvDiff::decode(&env_state_var_value.to_string_lossy())?;

                            let direnv_file = env::var_os(DIRENV_FILE_VAR_KEY)
                                .iter()
                                .map(PathBuf::from)
                                .collect::<Vec<_>>();

                            if rcs == direnv_file {
                                return Ok((rcs, direnv_file));
                            }

                            print_shell_export(args.shell, direnv_diff.get_env_vars_reset());

                            Ok((rcs, direnv_file))
                        },
                        |(rcs, direnv_file)| {
                            if rcs == direnv_file {
                                return Ok(());
                            }

                            let env_vars_state = rcs.into_iter().try_fold(
                                EnvVarsState::new(),
                                |mut acc, envrc| -> anyhow::Result<EnvVarsState> {
                                    acc.extend(get_export_env_vars_state(envrc)?);
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

fn find_envrc_walking_up_file_hierarchy(start_dir: PathBuf) -> Option<PathBuf> {
    start_dir.ancestors().find_map(|ancestor| {
        let envrc_path = ancestor.join(".envrc");
        fs::File::open(&envrc_path).ok().map(|_| envrc_path)
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DirenvDiff {
    p: EnvVars,
    n: EnvVars,
}

impl DirenvDiff {
    fn encode(&self) -> anyhow::Result<String> {
        let json = serde_json::to_vec(&self)?;
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&json)?;
        Ok(BASE64_URL_SAFE_NO_PAD.encode(&encoder.finish()?))
    }

    fn decode(encoded_direnv_diff: &str) -> anyhow::Result<Self> {
        let data = BASE64_URL_SAFE_NO_PAD.decode(encoded_direnv_diff)?;
        let mut decoder = ZlibDecoder::new(data.as_slice());
        let mut json = Vec::new();
        decoder.read_to_end(&mut json)?;
        Ok(serde_json::from_slice(&json)?)
    }

    fn get_env_vars_reset(self) -> EnvVarsState {
        get_env_vars_reset(
            self.p,
            self.n.keys().cloned().collect(),
            String::from(DIRENV_ENV_STATE_VAR_KEY),
        )
    }
}

fn get_export_env_vars_state(envrc: PathBuf) -> anyhow::Result<EnvVarsState> {
    let EnvVarUpdates {
        mut new_env_vars,
        old_env_vars_to_be_updated,
    } = get_new_env_vars(envrc)?;
    let direnv_diff = DirenvDiff {
        p: old_env_vars_to_be_updated,
        n: new_env_vars.clone(),
    };
    new_env_vars.insert(
        String::from(DIRENV_ENV_STATE_VAR_KEY),
        direnv_diff.encode()?,
    );
    Ok(env_vars_state_from_env_vars(new_env_vars))
}

struct EnvVarUpdates {
    new_env_vars: EnvVars,
    old_env_vars_to_be_updated: EnvVars,
}

fn get_new_env_vars(envrc: PathBuf) -> anyhow::Result<EnvVarUpdates> {
    let mut bash_env_vars = EnvVars::new();

    // MacOS ships with an ancient version of Bash. This allows using a newer version.
    let old_path = env::var_os(ENV_VAR_KEY_PATH).map(|p| String::from(p.to_string_lossy()));
    if let Some(path_value) = old_path.clone() {
        bash_env_vars.insert(String::from(ENV_VAR_KEY_PATH), path_value);
    }
    // Prints devshell "message of the day" the same way it would in `direnv`
    // https://github.com/numtide/devshell/blob/7c9e793ebe66bcba8292989a68c0419b737a22a0/modules/devshell.nix#L400
    bash_env_vars.insert(String::from("DIRENV_IN_ENVRC"), String::from("1"));

    let direnv_file = String::from(envrc.to_string_lossy());
    let mut new_env_vars = get_env_vars_from_bash(BashSource::File(envrc), Some(bash_env_vars))?;
    new_env_vars.insert(String::from(DIRENV_FILE_VAR_KEY), direnv_file);
    remove_ignored_env_vars(&mut new_env_vars);
    if new_env_vars.get(ENV_VAR_KEY_PATH) == old_path.as_ref() {
        new_env_vars.remove(ENV_VAR_KEY_PATH);
    }

    let old_env_vars_to_be_updated = {
        let mut old_env_vars = get_env_vars_from_current_process();
        remove_ignored_env_vars(&mut old_env_vars);
        get_old_env_vars_to_be_updated(old_env_vars, &new_env_vars)
    };

    if new_env_vars.contains_key(ENV_VAR_KEY_PATH) {
        merge_delimited_env_var(
            ENV_VAR_KEY_PATH,
            ':',
            ':',
            &old_env_vars_to_be_updated,
            &mut new_env_vars,
        );
    }
    if new_env_vars.contains_key(ENV_VAR_KEY_XDG_DATA_DIRS) {
        merge_delimited_env_var(
            ENV_VAR_KEY_XDG_DATA_DIRS,
            ':',
            ':',
            &old_env_vars_to_be_updated,
            &mut new_env_vars,
        );
    }

    Ok(EnvVarUpdates {
        new_env_vars,
        old_env_vars_to_be_updated,
    })
}

fn print_shell_export(shell: DirenvShell, env_vars_state: EnvVarsState) {
    let export = match shell {
        DirenvShell::Bash => {
            shells::bash::export(env_vars_state, Some(&SEMICOLON_DELIMITED_ENV_VARS))
        }
        DirenvShell::Fish => {
            shells::fish::export(env_vars_state, Some(&SEMICOLON_DELIMITED_ENV_VARS))
        }
        DirenvShell::Json => {
            shells::json::export(env_vars_state, Some(&SEMICOLON_DELIMITED_ENV_VARS))
        }
        DirenvShell::Zsh => {
            shells::zsh::export(env_vars_state, Some(&SEMICOLON_DELIMITED_ENV_VARS))
        }
    };
    println!("{}", export);
}
