mod config;
mod constants;
mod opt;
mod shell;

use clap::Parser;

use crate::opt::{
    Envoluntary, EnvoluntaryCommands, EnvoluntaryConfigCommands, EnvoluntaryShellCommands,
};

fn main() -> anyhow::Result<()> {
    let opt = Envoluntary::parse();

    match opt.command {
        EnvoluntaryCommands::Config { config } => match config {
            EnvoluntaryConfigCommands::PrintPath => {
                config::print_path()?;
            }
            EnvoluntaryConfigCommands::Edit(args) => {
                config::edit(args.config_path.as_deref(), args.editor_program.as_deref())?
            }
            EnvoluntaryConfigCommands::AddEntry(args) => {
                config::add_entry(
                    args.config_path.as_deref(),
                    args.pattern,
                    args.flake_reference,
                )?;
            }
            EnvoluntaryConfigCommands::PrintMatchingEntries(args) => {
                config::print_matching_entries(args.config_path.as_deref(), &args.path)?;
            }
        },
        EnvoluntaryCommands::Shell { shell } => match shell {
            EnvoluntaryShellCommands::CheckNixVersion => {
                nix_dev_env::check_nix_version()?;
            }
            EnvoluntaryShellCommands::Hook(args) => {
                shell::print_hook(args.shell)?;
            }
            EnvoluntaryShellCommands::Export(args) => {
                shell::print_export(args)?;
            }
            EnvoluntaryShellCommands::PrintCachePath(args) => {
                shell::print_cache_path(args)?;
            }
        },
    };

    Ok(())
}
