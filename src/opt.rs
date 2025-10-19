use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
pub struct Envoluntary {
    #[arg(long)]
    pub cache_dir: PathBuf,
    /// https://nix.dev/manual/nix/latest/command-ref/new-cli/nix3-flake#flake-references
    #[arg(long)]
    pub flake_reference: String,
    #[arg(long)]
    pub force_update: bool,
}
