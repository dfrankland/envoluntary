use std::{ffi::OsString, path::PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};

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
    Edit(EnvoluntaryConfigEditArgs),
    AddEntry(EnvoluntaryConfigAddEntryArgs),
    PrintMatchingEntries(EnvoluntaryConfigPrintMatchingEntriesArgs),
}

#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryConfigEditArgs {
    #[arg(long)]
    pub config_path: Option<PathBuf>,
    #[arg(long)]
    pub editor_program: Option<OsString>,
}

#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryConfigAddEntryArgs {
    pub pattern: String,
    pub flake_reference: String,
    #[arg(long)]
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryConfigPrintMatchingEntriesArgs {
    pub path: PathBuf,
    #[arg(long)]
    pub config_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum EnvoluntaryShellCommands {
    CheckNixVersion,
    Hook(EnvoluntaryShellHookArgs),
    Export(EnvoluntaryShellExportArgs),
    PrintCachePath(EnvoluntaryShellPrintCachePathArgs),
}

#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryShellHookArgs {
    pub shell: EnvoluntaryShell,
}

#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryShellExportArgs {
    pub shell: EnvoluntaryShell,
    #[arg(long)]
    pub config_path: Option<PathBuf>,
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,
    /// <https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake#flake-references>
    #[arg(long)]
    pub flake_references: Option<Vec<String>>,
    #[arg(long)]
    pub force_update: bool,
    #[arg(long)]
    pub current_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryShellPrintCachePathArgs {
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,
    /// <https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake#flake-references>
    #[arg(long)]
    pub flake_reference: String,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EnvoluntaryShell {
    Bash,
    Fish,
    Json,
    Zsh,
}
