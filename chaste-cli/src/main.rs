// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use argh::FromArgs;

mod audit;
mod why;

fn implem_from_name(name: &str) -> Result<chaste::Implementation, String> {
    chaste::Implementation::from_name(name)
        .ok_or_else(|| format!("Unknown implementation name: {name:?}"))
}

#[derive(FromArgs)]
/// Chaste.
struct Args {
    #[argh(subcommand)]
    subcommand: Subcommand,

    #[argh(option, from_str_fn(implem_from_name))]
    /// implementation whose lockfile should be checked
    implem: Option<chaste::Implementation>,

    #[argh(option)]
    /// directory to be checked
    cwd: Option<PathBuf>,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Subcommand {
    Audit(audit::Audit),
    Why(why::Why),
}

fn main() -> Result<()> {
    let args: Args = argh::from_env();

    let cwd = match args.cwd {
        Some(p) => p,
        None => std::env::current_dir()?,
    };
    let chastefile = if let Some(implem) = args.implem {
        chaste::from_root_path_with_implementation(&cwd, implem)
    } else {
        chaste::from_root_path(&cwd)
    }
    .with_context(|| format!("Could not parse the lockfile from {cwd:?}"))?;

    match args.subcommand {
        Subcommand::Audit(audit) => audit::run(audit, chastefile),
        Subcommand::Why(why) => why::run(why, chastefile),
    }
}
