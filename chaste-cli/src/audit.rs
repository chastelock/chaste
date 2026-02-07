// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use anyhow::Result;
use argh::FromArgs;
use chaste::types::ssri::Algorithm;
use chaste::types::ProviderMeta;
use chaste::Package;

#[derive(FromArgs)]
#[argh(subcommand, name = "audit")]
/// Potential problems with your dependency tree
pub struct Audit {
    #[argh(switch)]
    /// failing checks should not result in non-zero exit code
    failures_ok: bool,
}

/// A specific check performed in the audit
struct Kruisje<'a> {
    desc: &'a str,
    failed: Vec<&'a Package>,
}

pub fn run(sub: Audit, chastefile: chaste::Chastefile<chaste::Meta>) -> Result<()> {
    let meta = chastefile.meta();
    print!("Checked a {} ", meta.provider_name());
    if let Some(lv) = meta.lockfile_version() {
        print!("({lv}) ");
    }
    println!("lockfile.");

    let mut checksumless = Vec::new();
    let mut insufficient_checksums = Vec::new();
    let mut unknown_source = Vec::new();
    let root_pid = chastefile.root_package_id();
    let member_pids = chastefile.workspace_member_ids();
    for (pid, package) in chastefile.packages_with_ids() {
        if pid == root_pid || member_pids.contains(&pid) {
            continue;
        }
        let maybe_checksums = package.checksums();
        if let Some(checksums) = maybe_checksums {
            if ![Algorithm::Sha512, Algorithm::Sha384, Algorithm::Sha256]
                .contains(&checksums.integrity().pick_algorithm())
            {
                insufficient_checksums.push(package);
            }
        } else {
            checksumless.push(package);
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

    if !sub.failures_ok {
        std::process::exit(failed_kruisjes.try_into()?);
    }
    Ok(())
}
