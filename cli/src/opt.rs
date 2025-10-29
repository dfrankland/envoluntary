use std::{ffi::OsString, path::PathBuf};

use clap::{Args, Parser, Subcommand, ValueEnum};

/// A Nix flake-based development environment manager for automatic shell integration.
///
/// Envoluntary automatically loads Nix development environments based on directory patterns
/// using flake references, integrating seamlessly with your shell.
#[derive(Debug, Clone, Parser)]
#[command(version, about)]
pub struct Envoluntary {
    #[command(subcommand)]
    pub command: EnvoluntaryCommands,
}

/// Top-level commands for managing configuration and shell integration.
#[derive(Debug, Clone, Subcommand)]
pub enum EnvoluntaryCommands {
    /// Manage Envoluntary configuration.
    ///
    /// These commands help you set up and manage your configuration file,
    /// which defines which directory patterns map to which Nix flake references.
    Config {
        #[command(subcommand)]
        config: EnvoluntaryConfigCommands,
    },
    /// Manage shell integration and environment exports.
    ///
    /// These commands generate shell hooks and export environment variables
    /// based on the current directory and configuration.
    Shell {
        #[command(subcommand)]
        shell: EnvoluntaryShellCommands,
    },
}

/// Configuration management subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum EnvoluntaryConfigCommands {
    /// Print the path to the configuration file.
    ///
    /// Displays the full path to the Envoluntary configuration file location.
    /// The configuration file is stored at `$XDG_CONFIG_HOME/envoluntary/config.toml`
    /// (or `~/.config/envoluntary/config.toml` if `$XDG_CONFIG_HOME` is not set).
    PrintPath,

    /// Open the configuration file in your default editor.
    ///
    /// Launches the editor specified by the `$EDITOR` environment variable
    /// (or a custom editor via `--editor-program`) to edit the configuration file.
    Edit(EnvoluntaryConfigEditArgs),

    /// Add a new entry to the configuration file.
    ///
    /// Adds a mapping from a directory pattern (regex) to a Nix flake reference.
    /// When you're in a directory matching the pattern, Envoluntary will automatically
    /// load the environment defined by that flake reference.
    AddEntry(EnvoluntaryConfigAddEntryArgs),

    /// Print configuration entries that match a given path.
    ///
    /// Shows which configuration entries (patterns and their corresponding flake references)
    /// match the specified directory path. Useful for debugging which environments will be loaded.
    PrintMatchingEntries(EnvoluntaryConfigPrintMatchingEntriesArgs),
}

/// Arguments for the `config edit` command.
#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryConfigEditArgs {
    /// Path to the configuration file to edit (overrides default location).
    ///
    /// If not provided, uses the default configuration path.
    #[arg(long)]
    pub config_path: Option<PathBuf>,

    /// Program to use for editing the configuration file (overrides `$EDITOR`).
    ///
    /// If not provided, uses the `$EDITOR` environment variable.
    #[arg(long)]
    pub editor_program: Option<OsString>,
}

/// Arguments for the `config add-entry` command.
#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryConfigAddEntryArgs {
    /// A regex pattern to match against directory paths.
    ///
    /// This pattern is matched against the full path of the current directory.
    /// When a directory path matches this pattern, the associated flake reference will be used.
    pub pattern: String,

    /// A Nix flake reference to load when the pattern matches.
    ///
    /// This can be a local flake path (e.g., `./flake.nix`),
    /// or a remote flake reference (e.g., `github:owner/repo`).
    /// See: <https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake#flake-references>
    pub flake_reference: String,

    /// Whether to evaluate the flake in impure mode.
    ///
    /// If set to `true`, Nix will evaluate the flake with `--impure`, allowing access to environment variables
    /// and other non-deterministic inputs. If not provided, uses the default evaluation mode.
    #[arg(long)]
    pub impure: Option<bool>,

    /// Path to the configuration file (overrides default location).
    ///
    /// If not provided, uses the default configuration path.
    #[arg(long)]
    pub config_path: Option<PathBuf>,
}

/// Arguments for the `config print-matching-entries` command.
#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryConfigPrintMatchingEntriesArgs {
    /// The directory path to match against configuration patterns.
    pub path: PathBuf,

    /// Path to the configuration file (overrides default location).
    ///
    /// If not provided, uses the default configuration path.
    #[arg(long)]
    pub config_path: Option<PathBuf>,
}

/// Shell integration subcommands.
#[derive(Debug, Clone, Subcommand)]
pub enum EnvoluntaryShellCommands {
    /// Check if the installed Nix version is compatible.
    ///
    /// Verifies that your Nix installation meets the minimum version requirements
    /// for Envoluntary to function correctly.
    CheckNixVersion,

    /// Print shell hook code for the specified shell.
    ///
    /// Generates initialization code that should be added to your shell's configuration
    /// (e.g., `.bashrc`, `config.fish`, or `.zshrc`) to enable automatic environment loading.
    /// This hook will be called whenever you enter a new directory.
    Hook(EnvoluntaryShellHookArgs),

    /// Export environment variables for the current directory.
    ///
    /// Generates shell commands to export environment variables based on matching
    /// configuration entries for the current directory. This is called by the shell hook.
    Export(EnvoluntaryShellExportArgs),

    /// Print the cache path for a given Nix flake reference.
    ///
    /// Shows where Envoluntary caches the compiled profiles for a specific flake reference.
    /// Useful for debugging cache-related issues.
    PrintCachePath(EnvoluntaryShellPrintCachePathArgs),
}

/// Arguments for the `shell hook` command.
#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryShellHookArgs {
    /// The shell for which to generate the hook code.
    ///
    /// The hook code syntax varies by shell (bash, fish, zsh).
    pub shell: EnvoluntaryShell,
}

/// Arguments for the `shell export` command.
#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryShellExportArgs {
    /// The shell for which to generate export code.
    ///
    /// The export syntax varies by shell (bash, fish, zsh, or JSON).
    pub shell: EnvoluntaryShell,

    /// Path to the configuration file (overrides default location).
    ///
    /// If not provided, uses the default configuration path.
    #[arg(long)]
    pub config_path: Option<PathBuf>,

    /// Directory for caching Nix profiles (overrides default cache location).
    ///
    /// If not provided, uses `$XDG_CACHE_HOME/envoluntary` (or `~/.cache/envoluntary` if not set).
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,

    /// Explicit list of Nix flake references to load (overrides config-based matching).
    ///
    /// If provided, these flake references will be used instead of matching against
    /// the configuration file patterns. Useful for testing or temporary overrides.
    /// See: <https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake#flake-references>
    #[arg(long)]
    pub flake_references: Option<Vec<String>>,

    /// Override whether to evaluate the flake in impure mode.
    ///
    /// If set to `true`, Nix will evaluate the flake with `--impure`, allowing access to environment variables
    /// and other non-deterministic inputs. If not provided, uses the default evaluation mode.
    pub impure: Option<bool>,

    /// Force update of cached Nix profiles.
    ///
    /// If set, Envoluntary will rebuild the Nix profiles even if cached versions exist.
    /// Useful when your flake.nix has changed and you want to reload immediately.
    #[arg(long)]
    pub force_update: bool,

    /// The directory path to check for matching configuration entries (for testing).
    ///
    /// If not provided, uses the current working directory.
    /// Useful for debugging what environments would be loaded in a specific directory.
    #[arg(long)]
    pub current_dir: Option<PathBuf>,
}

/// Arguments for the `shell print-cache-path` command.
#[derive(Debug, Clone, Args)]
pub struct EnvoluntaryShellPrintCachePathArgs {
    /// The Nix flake reference to get the cache path for.
    ///
    /// See: <https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake#flake-references>
    #[arg(long)]
    pub flake_reference: String,

    /// Directory for caching Nix profiles (overrides default cache location).
    ///
    /// If not provided, uses `$XDG_CACHE_HOME/envoluntary` (or `~/.cache/envoluntary` if not set).
    #[arg(long)]
    pub cache_dir: Option<PathBuf>,
}

/// Supported shells for hook and export code generation.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum EnvoluntaryShell {
    /// POSIX shell compatible syntax (sh, bash).
    Bash,
    /// Fish shell syntax.
    Fish,
    /// JSON output format (useful for machine parsing).
    Json,
    /// Z shell (zsh) syntax.
    Zsh,
}
