mod opt;
mod shell;

use clap::Parser;

use crate::opt::{Direnv, DirenvCommands};

fn main() -> anyhow::Result<()> {
    let opt = Direnv::parse();

    match opt.command {
        DirenvCommands::Hook(args) => {
            shell::print_hook(args.shell)?;
        }
        DirenvCommands::Export(args) => {
            shell::print_export(args)?;
        }
    };

    Ok(())
}
