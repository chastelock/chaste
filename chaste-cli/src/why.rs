// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use anyhow::{Context, Result};
use argh::FromArgs;
use chaste::{Dependency, PackageID};

#[derive(FromArgs)]
#[argh(subcommand, name = "why")]
/// Why is this package here?
pub struct Why {
    #[argh(positional)]
    /// package name
    package_name: String,
}

fn permute<'a, G>(preceding: Vec<&'a Dependency>, generator: G) -> Vec<Vec<&'a Dependency>>
where
    G: Fn(PackageID) -> Vec<&'a Dependency> + Clone,
{
    let last = preceding.last().unwrap();
    let generated = generator(last.from);
    match generated.len() {
        0 => vec![preceding],
        1 => {
            let mut res = preceding;
            let g = generated.first().unwrap();
            if res.iter().any(|d| d.from == g.from || d.on == g.on) {
                return Vec::new();
            }
            res.push(g);
            permute(res, generator)
        }
        _ => {
            let mut res = Vec::new();
            for g in generated {
                let mut branch = preceding.clone();
                if branch.iter().any(|d| d.from == g.from || d.on == g.on) {
                    continue;
                }
                branch.push(g);
                res.extend(permute(branch, generator.clone()));
            }
            res
        }
    }
}

pub fn run(sub: Why) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let chastefile = chaste::from_root_path(&cwd)
        .with_context(|| format!("Could not parse the lockfile from {cwd:?}"))?;

    let packages = chastefile
        .packages_with_ids()
        .into_iter()
        .filter(|(_, pkg)| pkg.name().is_some_and(|n| n == sub.package_name));
    let mut permutations = Vec::new();
    for (current_pid, _package) in packages {
        for dep in chastefile.package_dependents(current_pid) {
            permutations.extend(permute(vec![dep], |pid| chastefile.package_dependents(pid)));
        }
    }
    for mut permut in permutations {
        permut.reverse();
        let initial_pkg = chastefile.package(permut.first().unwrap().from);
        print!(
            "{}",
            initial_pkg
                .name()
                .map(|n| n.as_ref())
                .unwrap_or("[unnamed]")
        );
        for d in permut {
            let pkg = chastefile.package(d.on);
            print!(
                " -{:?}-> {}",
                d.kind,
                pkg.name().map(|n| n.as_ref()).unwrap_or("[unnamed]")
            );
        }
        println!();
    }

    Ok(())
}
