use std::{ffi::OsString, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Clone, Parser)]
#[command(version, about, long_about = None)]
pub struct Envoluntary {
    #[command(subcommand)]
    pub command: Option<EnvoluntaryCommands>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum EnvoluntaryCommands {
    Config {
        #[command(subcommand)]
        config: EnvoluntaryConfigCommands,
    },
    Shell {
        #[command(subcommand)]
        shell: EnvoluntaryShellCommands,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum EnvoluntaryConfigCommands {
    PrintPath,
    Edit {
        #[arg(long)]
        config_path: Option<PathBuf>,
        #[arg(long)]
        editor_program: Option<OsString>,
    },
    AddEntry {
        pattern: String,
        flake_reference: String,
        #[arg(long)]
        config_path: Option<PathBuf>,
    },
    PrintMatchingEntries {
        path: PathBuf,
        #[arg(long)]
        config_path: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum EnvoluntaryShellCommands {
    Hook {
        shell: EnvoluntaryShell,
    },
    Export {
        shell: EnvoluntaryShell,
        #[arg(long)]
        config_path: Option<PathBuf>,
        #[arg(long)]
        cache_dir: Option<PathBuf>,
        /// <https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake#flake-references>
        #[arg(long)]
        flake_references: Option<Vec<String>>,
        #[arg(long)]
        force_update: bool,
        #[arg(long)]
        current_dir: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, ValueEnum)]
pub enum EnvoluntaryShell {
    Fish,
}
