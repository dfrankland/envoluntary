mod nix_command;
mod nix_profile_cache;
mod nix_version_check;
mod opt;

use clap::Parser;

use crate::{
    nix_profile_cache::NixProfileCache, nix_version_check::check_nix_version, opt::Envoluntary,
};

fn main() -> anyhow::Result<()> {
    let opt = Envoluntary::parse();

    check_nix_version()?;

    let cache_profile = NixProfileCache::new(opt.cache_dir, opt.flake_reference)?;

    if opt.force_update || cache_profile.needs_update()? {
        cache_profile.update()?;
    }

    cache_profile.print()?;

    Ok(())
}
