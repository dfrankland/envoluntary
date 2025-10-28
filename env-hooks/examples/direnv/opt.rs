use clap::{Args, Parser, Subcommand, ValueEnum};

/// direnv is an extension for your shell.
///
/// It augments existing shells with a new feature that can load and unload environment variables depending on the current directory.
#[derive(Debug, Clone, Parser)]
#[command(version, about)]
pub struct Direnv {
    #[command(subcommand)]
    pub command: DirenvCommands,
}

#[derive(Debug, Clone, Subcommand)]
pub enum DirenvCommands {
    Hook(DirenvShellHookArgs),
    Export(DirenvShellExportArgs),
}

/// Used to setup the shell hook
#[derive(Debug, Clone, Args)]
pub struct DirenvShellHookArgs {
    pub shell: DirenvShell,
}

/// Loads an .envrc or .env and prints the diff in terms of exports.
#[derive(Debug, Clone, Args)]
pub struct DirenvShellExportArgs {
    pub shell: DirenvShell,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DirenvShell {
    Bash,
    Fish,
    Json,
    Zsh,
}
