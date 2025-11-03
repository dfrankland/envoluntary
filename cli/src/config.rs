use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use duct::cmd;
use env_hooks::state;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::constants::CLI_NAME;

pub fn print_path() -> anyhow::Result<()> {
    println!("{}", get_config_path(None)?.display());
    Ok(())
}

pub fn edit(
    provided_config_path: Option<&Path>,
    provided_editor_program: Option<&OsStr>,
) -> anyhow::Result<()> {
    let config_path = get_config_path(provided_config_path)?;
    let editor_program = if let Some(editor_program) = provided_editor_program {
        editor_program
    } else {
        &env::var_os("EDITOR")
            .ok_or_else(|| anyhow::anyhow!("Couldn't find $EDITOR for config."))?
    };
    cmd!(editor_program, config_path).start()?.wait()?;
    Ok(())
}

pub fn add_entry(
    provided_config_path: Option<&Path>,
    pattern: Option<String>,
    file_pattern: Option<String>,
    flake_reference: String,
    impure: Option<bool>,
) -> anyhow::Result<()> {
    let pattern = if let Some(pattern) = pattern {
        Some(Regex::new(&pattern)?)
    } else {
        None
    };
    let file_pattern = if let Some(file_pattern) = file_pattern {
        Some(Regex::new(&file_pattern)?)
    } else {
        None
    };
    let entry = ConfigEntry {
        pattern,
        file_pattern,
        config: Config {
            flake_reference,
            impure,
        },
    };
    let config_path = get_config_path(provided_config_path)?;
    let mut envoluntary_config = EnvoluntaryConfig::load(&config_path)?;
    if let Some(ref mut entries) = envoluntary_config.entries {
        entries.push(entry);
    } else {
        envoluntary_config.entries = Some(vec![entry]);
    }
    envoluntary_config.save(&config_path)?;
    Ok(())
}

pub fn print_matching_entries(
    provided_config_path: Option<&Path>,
    path: &Path,
) -> anyhow::Result<()> {
    let config_path = get_config_path(provided_config_path)?;
    let envoluntary_config = EnvoluntaryConfig::load(&config_path)?;
    for entry in envoluntary_config.matching_entries(path) {
        println!("{}", serde_json::to_string(&entry)?);
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnvoluntaryConfig {
    entries: Option<Vec<ConfigEntry>>,
}

impl EnvoluntaryConfig {
    pub fn load(config_path: &Path) -> anyhow::Result<Self> {
        if !config_path.exists() {
            return Ok(EnvoluntaryConfig::default());
        }

        let envoluntary_config = config::Config::builder()
            .add_source(config::File::from(config_path))
            .build()?
            .try_deserialize::<EnvoluntaryConfig>()?;

        Ok(envoluntary_config)
    }

    pub fn save(&self, config_path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let contents = toml::to_string_pretty(&self)?;
        fs::write(config_path, contents)?;

        Ok(())
    }

    pub fn matching_entries(&self, path: &Path) -> Vec<ConfigEntry> {
        self.entries
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .filter(|entry| {
                let dir_match = entry
                    .pattern
                    .as_ref()
                    .map(|x| x.is_match(&path.to_string_lossy()));
                let file_match = entry.file_pattern.as_ref().map(|pattern| {
                    state::ShellPromptState::check_files_backwards(path, |filename| {
                        pattern.is_match(filename)
                    })
                });
                match (dir_match, file_match) {
                    (Some(dir_match), Some(file_match)) => dir_match && file_match,
                    (None, Some(file_match)) => file_match,
                    (Some(dir_match), None) => dir_match,
                    _ => false,
                }
            })
            .cloned()
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigEntry {
    #[serde(default, with = "serde_regex")]
    pub pattern: Option<Regex>,
    #[serde(default, with = "serde_regex")]
    pub file_pattern: Option<Regex>,
    #[serde(flatten)]
    pub config: Config,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub flake_reference: String,
    pub impure: Option<bool>,
}

pub fn get_config_path(provided_config_path: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(config_path) = provided_config_path {
        return Ok(PathBuf::from(config_path));
    }
    Ok(get_config_home_dir()?.join(CLI_NAME).join("config.toml"))
}

fn get_config_home_dir() -> anyhow::Result<PathBuf> {
    if let Some(xdg_config_home) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg_config_home));
    }

    let home = get_home_dir()?;
    Ok(home.join(".config"))
}

pub fn get_cache_dir(provided_cache_dir: Option<&Path>) -> anyhow::Result<PathBuf> {
    if let Some(cache_dir) = provided_cache_dir {
        return Ok(PathBuf::from(cache_dir));
    }
    Ok(get_cache_home_dir()?.join(CLI_NAME))
}

fn get_cache_home_dir() -> anyhow::Result<PathBuf> {
    if let Some(xdg_cache_home) = env::var_os("XDG_CACHE_HOME") {
        return Ok(PathBuf::from(xdg_cache_home));
    }

    let home = get_home_dir()?;
    Ok(home.join(".cache"))
}

fn get_home_dir() -> anyhow::Result<PathBuf> {
    env::home_dir().ok_or_else(|| anyhow::anyhow!("Couldn't find $HOME for config."))
}
