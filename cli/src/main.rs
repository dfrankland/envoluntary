mod opt;

use std::{collections::BTreeMap, path::PathBuf};

use clap::Parser;
use env_hooks::{
    BashSource, get_env_vars_from_bash, get_env_vars_from_current_process,
    get_old_env_vars_to_be_updated, merge_delimited_env_var,
};
use nix_dev_env::{NixProfileCache, check_nix_version};

use crate::opt::Envoluntary;

fn main() -> anyhow::Result<()> {
    let opt = Envoluntary::parse();

    check_nix_version()?;

    let cache_profile = NixProfileCache::new(opt.cache_dir, opt.flake_reference)?;

    if opt.force_update || cache_profile.needs_update()? {
        cache_profile.update()?;
    }

    let mut new_env_vars = {
        let mut bash_env_vars = BTreeMap::new();
        // Prints devshell "message of the day" the same way it would in `direnv`
        // https://github.com/numtide/devshell/blob/7c9e793ebe66bcba8292989a68c0419b737a22a0/modules/devshell.nix#L400
        bash_env_vars.insert(String::from("DIRENV_IN_ENVRC"), String::from("1"));
        get_env_vars_from_bash(
            BashSource::File(PathBuf::from(cache_profile.profile_rc())),
            Some(bash_env_vars),
        )?
    };

    let old_env_vars_to_be_updated = {
        let old_env_vars = get_env_vars_from_current_process();
        get_old_env_vars_to_be_updated(old_env_vars, &new_env_vars)
    };

    merge_delimited_env_var(
        "PATH",
        ':',
        ':',
        &old_env_vars_to_be_updated,
        &mut new_env_vars,
    );
    merge_delimited_env_var(
        "XDG_DATA_DIRS",
        ':',
        ':',
        &old_env_vars_to_be_updated,
        &mut new_env_vars,
    );

    dbg!(new_env_vars, old_env_vars_to_be_updated);

    Ok(())
}
