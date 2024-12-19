// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use anyhow::Result;
use argh::FromArgs;

mod why;

#[derive(FromArgs)]
/// Chaste.
struct Args {
    #[argh(subcommand)]
    subcommand: Subcommand,
}

#[derive(FromArgs)]
#[argh(subcommand)]
enum Subcommand {
    Why(why::Why),
}

fn main() -> Result<()> {
    let args: Args = argh::from_env();
    match args.subcommand {
        Subcommand::Why(why) => why::run(why),
    }
}
