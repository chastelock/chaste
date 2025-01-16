// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use anyhow::{Context, Result};
use argh::FromArgs;
use chaste::types::ssri::Algorithm;
use chaste::Package;

#[derive(FromArgs)]
#[argh(subcommand, name = "audit")]
/// Potential problems with your dependency tree
pub struct Audit {}

/// A specific check performed in the audit
struct Kruisje<'a> {
    desc: &'a str,
    failed: Vec<&'a Package>,
}

pub fn run(_sub: Audit) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let chastefile = chaste::from_root_path(&cwd)
        .with_context(|| format!("Could not parse the lockfile from {cwd:?}"))?;

    let mut checksumless = Vec::new();
    let mut insufficient_checksums = Vec::new();
    let mut unknown_source = Vec::new();
    let root_pid = chastefile.root_package_id();
    let member_pids = chastefile.workspace_member_ids();
    for (pid, package) in chastefile.packages_with_ids() {
        if pid == root_pid || member_pids.contains(&pid) {
            continue;
        }
        let integrity = package.integrity();
        if integrity.hashes.is_empty() {
            checksumless.push(package);
        } else if ![Algorithm::Sha512, Algorithm::Sha384, Algorithm::Sha256]
            .contains(&integrity.pick_algorithm())
        {
            insufficient_checksums.push(package);
        }
        if package.source().is_none() {
            unknown_source.push(package);
        }
    }

    let kruisjes = [
        Kruisje {
            desc: "no checksums",
            failed: checksumless,
        },
        Kruisje {
            desc: "insecure checksums",
            failed: insufficient_checksums,
        },
        Kruisje {
            desc: "unrecognized source",
            failed: unknown_source,
        },
    ];

    let failed_kruisjes = kruisjes.iter().filter(|k| !k.failed.is_empty()).count();
    if failed_kruisjes == 0 {
        print!("All good! ")
    }
    println!("Out of {} dependencies:", chastefile.packages().len());
    for kruisje in &kruisjes {
        if kruisje.failed.is_empty() {
            println!("✅ No packages with {}", kruisje.desc);
        }
    }
    for kruisje in &kruisjes {
        if !kruisje.failed.is_empty() {
            let len = kruisje.failed.len();
            let mut list: Vec<&str> = kruisje
                .failed
                .iter()
                .map(|p| p.name().map(|n| n.as_ref()).unwrap_or("[unnamed]"))
                .collect();
            list.sort_unstable();
            println!(
                "❌ {} package{} with {}:\n\t{}",
                len,
                if len == 1 { "" } else { "s" },
                kruisje.desc,
                list.join(" ")
            );
        }
    }

    std::process::exit(failed_kruisjes.try_into()?);
}
