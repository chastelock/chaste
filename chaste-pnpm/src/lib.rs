// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::path::Path;

use chaste_types::{
    package_name_str, ssri, Chastefile, ChastefileBuilder, Checksums, DependencyBuilder,
    DependencyKind, InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageDerivation,
    PackageDerivationMetaBuilder, PackageID, PackageName, PackagePatchBuilder, PackageSource,
    SourceVersionSpecifier, PACKAGE_JSON_FILENAME,
};
use nom::branch::alt;
use nom::bytes::complete::{tag, take};
use nom::combinator::{eof, recognize, rest};
use nom::sequence::{delimited, terminated};
use nom::Parser;

pub use crate::error::Error;
use crate::error::Result;

mod error;
#[cfg(test)]
mod tests;
mod types;

pub static LOCKFILE_NAME: &str = "pnpm-lock.yaml";

#[allow(clippy::type_complexity)]
fn snapshot_key_rest<'a>(
    snap_pid: &BTreeMap<&'a str, PackageID>,
    desc_pid: &BTreeMap<
        (&'a str, &'a str),
        (
            PackageID,
            &HashMap<Cow<'a, str>, Cow<'a, str>>,
            &HashMap<Cow<'a, str>, types::lock::PeerDependencyMeta>,
        ),
    >,
    rest: &'a str,
) -> Option<Vec<&'a str>> {
    let Ok((_, snap_pkg_name)) = delimited(tag("("), package_name_str, tag("@")).parse(rest) else {
        return None;
    };
    for (snap_key, _) in snap_pid.range(snap_pkg_name..) {
        if !snap_key.starts_with(snap_pkg_name) {
            break;
        }
        if let Ok((more_snap_rest, _)) =
            (tag("("), tag::<&str, &str, ()>(*snap_key), tag(")")).parse(rest)
        {
            let mut peers = vec![*snap_key];
            if more_snap_rest.is_empty() {
                return Some(peers);
            } else if let Some(mut more_peers) =
                snapshot_key_rest(snap_pid, desc_pid, more_snap_rest)
            {
                peers.append(&mut more_peers);
                return Some(peers);
            }
        }
    }

    // Failed to find a snapshot by snapshot key. But the dependency may be circular.
    // In that case:
    // 1) `snap_rest` contains a package key instead of a snapshot key,
    //    because that would be an infinite loop. (The rest is always gone,
    //    even if there are non-circular peer dependencies.)
    // 2) The snapshot may not exist in `snap_pid` due to chicken and egg problem.
    //
    // Therefore, check `desc_pid` for a package descriptor.
    for (desc_key, _) in desc_pid.range((snap_pkg_name, "")..) {
        let (desc_pn, desc_svd) = desc_key;
        if *desc_pn != snap_pkg_name {
            break;
        }
        if let Ok((more_snap_rest, (_, desc_str, _))) = (
            tag("("),
            recognize((tag::<&str, &str, ()>(*desc_pn), tag("@"), tag(*desc_svd))),
            tag(")"),
        )
            .parse(rest)
        {
            let mut peers = vec![desc_str];
            if more_snap_rest.is_empty() {
                return Some(peers);
            } else if let Some(mut more_peers) =
                snapshot_key_rest(snap_pid, desc_pid, more_snap_rest)
            {
                peers.append(&mut more_peers);
                return Some(peers);
            }
        }
    }

    None
}

pub fn parse<P>(root_dir: P) -> Result<Chastefile>
where
    P: AsRef<Path>,
{
    let root_dir = root_dir.as_ref();

    let lockfile_contents = fs::read_to_string(root_dir.join(LOCKFILE_NAME))?;
    let lockfile: types::Lockfile = serde_norway::from_str(&lockfile_contents)?;

    if lockfile.lockfile_version != "9.0" {
        return Err(Error::UnknownLockfileVersion(
            lockfile.lockfile_version.to_string(),
        ));
    }

    let mut chastefile = ChastefileBuilder::new();

    let mut importer_to_pid = HashMap::with_capacity(lockfile.importers.len());
    for importer_path in lockfile.importers.keys() {
        let package_json_contents = fs::read_to_string(if *importer_path == "." {
            root_dir.join(PACKAGE_JSON_FILENAME)
        } else {
            root_dir.join(importer_path).join(PACKAGE_JSON_FILENAME)
        })?;
        let package_json: types::PackageJson = serde_json::from_str(&package_json_contents)?;
        let importer_pkg = PackageBuilder::new(
            package_json
                .name
                .map(|n| PackageName::new(n.to_string()))
                .transpose()?,
            package_json.version.map(|v| v.to_string()),
        );
        let importer_pid = chastefile.add_package(importer_pkg.build()?)?;
        if *importer_path == "." {
            chastefile.set_root_package_id(importer_pid)?;
        } else {
            chastefile.set_as_workspace_member(importer_pid)?;
        }
        importer_to_pid.insert(importer_path, importer_pid);
        let installation = InstallationBuilder::new(
            importer_pid,
            ModulePath::new(if *importer_path == "." {
                "".to_string()
            } else {
                importer_path.to_string()
            })?,
        )
        .build()?;
        chastefile.add_package_installation(installation);
    }

    let mut desc_pid = BTreeMap::new();
    for (pkg_desc, pkg) in &lockfile.packages {
        let (_, (package_name, _, package_svd)) = (package_name_str, tag("@"), rest)
            .parse(pkg_desc)
            .map_err(|_| Error::InvalidPackageDescriptor(pkg_desc.to_string()))?;
        let version = pkg
            .version
            .as_deref()
            .or(Some(package_svd))
            .map(|v| v.to_string());
        let mut package =
            PackageBuilder::new(Some(PackageName::new(package_name.to_string())?), version);
        if let Some(integrity) = pkg.resolution.integrity {
            let inte: Integrity = integrity.parse()?;
            if !inte.hashes.is_empty() {
                package.checksums(Checksums::Tarball(inte));
            }
        }
        if let Some(tarball_url) = &pkg.resolution.tarball {
            // If there is a checksum, it's a custom registry.
            if pkg.resolution.integrity.is_some() {
                package.source(PackageSource::Npm);
            } else {
                package.source(PackageSource::TarballURL {
                    url: tarball_url.to_string(),
                });
            }
        } else if let Some(git_url) = package_svd.strip_prefix("git+") {
            package.source(PackageSource::Git {
                url: git_url.to_string(),
            });
        } else if SourceVersionSpecifier::new(package_svd.to_string()).is_ok_and(|svs| svs.is_npm())
        {
            package.source(PackageSource::Npm);
        }
        let pkg_pid = chastefile.add_package(package.build()?)?;
        desc_pid.insert(
            (package_name, package_svd),
            (pkg_pid, &pkg.peer_dependencies, &pkg.peer_dependencies_meta),
        );
    }

    let mut patch_store = HashMap::with_capacity(lockfile.patched_dependencies.len());
    for (k, v) in &lockfile.patched_dependencies {
        let Ok((_, (pn, _))) = (package_name_str, alt((eof, (tag("@"))))).parse(k) else {
            return Err(Error::InvalidPatchedPackageSpecifier(k.to_string()));
        };
        if v.hash.len() != 64 {
            return Err(Error::InvalidPatchHash(v.hash.to_string()));
        }
        let integrity = Integrity::from_hex(v.hash, ssri::Algorithm::Sha256)?;
        let mut patch = PackagePatchBuilder::new(v.path.to_string());
        patch.integrity(integrity);
        patch_store.insert((pn, v.hash), patch.build()?);
    }

    let mut snap_pid = BTreeMap::new();
    let mut pid_peers = HashMap::new();
    let mut snap_queue = VecDeque::from_iter(lockfile.snapshots.keys());
    let mut lap_i = 0usize;
    'queue: while let Some(pkg_desc) = snap_queue.pop_front() {
        let Some((snap_rest, pkg_name)) =
            terminated(package_name_str, tag("@")).parse(pkg_desc).ok()
        else {
            return Err(Error::InvalidPackageDescriptor(pkg_desc.to_string()));
        };
        // Not a peer dep: "@chastelock/package@1.0.0" snapshot for the ("@chastelock/package", "1.0.0") package.
        if let Some(&(pid, peer_deps, peers_meta)) = desc_pid.get(&(pkg_name, snap_rest)) {
            snap_pid.insert(pkg_desc.as_ref(), pid);
            pid_peers.insert(pid, (peer_deps, peers_meta));
            lap_i = 0;
            continue 'queue;
        }
        // Looking through descriptors to find a matching package.
        for ((d_pkg_name, d_pkg_svd), (mut pid, peer_deps, peers_meta)) in
            desc_pid.range((pkg_name, "")..)
        {
            // List is sorted alphabetically.
            if *d_pkg_name != pkg_name {
                break;
            }
            // A key like "react-router@7.2.0(react-dom@19.0.0(react@19.0.0))(react@19.0.0)", is now matched
            // to the ("react-router", "7.2.0") package.
            let Some(mut peers_suffix) = snap_rest.strip_prefix(d_pkg_svd) else {
                continue;
            };
            // If a package is patched with a diff over the original source,
            // handle the patch SHA-256 hex in the key.
            if let Ok((suff, patch_hash)) = delimited(
                tag::<&str, &str, ()>("(patch_hash="),
                take(64usize),
                tag(")"),
            )
            .parse(peers_suffix)
            {
                let Some(patch) = patch_store.get(&(pkg_name, patch_hash)) else {
                    return Err(Error::InvalidPatchHash(patch_hash.to_string()));
                };
                let patch_deriv_meta =
                    PackageDerivationMetaBuilder::new(PackageDerivation::Patch(patch.clone()), pid)
                        .build()?;
                let mut patched_pkg = PackageBuilder::new(
                    Some(PackageName::new(pkg_name.to_string()).unwrap()),
                    Some(d_pkg_svd.to_string()),
                );
                patched_pkg.derived(patch_deriv_meta);
                pid = chastefile.add_package(patched_pkg.build()?)?;

                if suff.is_empty() {
                    snap_pid.insert(pkg_desc, pid);
                    lap_i = 0;
                    continue 'queue;
                }
                peers_suffix = suff;
            }
            // Now we handle the "(react-dom@19.0.0(react@19.0.0))(react@19.0.0)" part.
            // These are matches not with packages, but with other snapshots.
            let Some(_peers) = snapshot_key_rest(&snap_pid, &desc_pid, peers_suffix) else {
                continue;
            };
            snap_pid.insert(pkg_desc, pid);
            pid_peers.insert(pid, (peer_deps, peers_meta));
            lap_i = 0;
            continue 'queue;
        }
        if lap_i > snap_queue.len() {
            return Err(Error::InvalidSnapshotDescriptor(pkg_desc.to_string()));
        }
        lap_i += 1;
        snap_queue.push_back(pkg_desc);
    }
    for (importer_path, importer) in &lockfile.importers {
        let importer_pid = *importer_to_pid.get(importer_path).unwrap();
        for (dependencies, kind) in [
            (&importer.dependencies, DependencyKind::Dependency),
            (&importer.dev_dependencies, DependencyKind::DevDependency),
            (&importer.peer_dependencies, DependencyKind::PeerDependency),
            (
                &importer.optional_dependencies,
                DependencyKind::OptionalDependency,
            ),
        ] {
            for (dep_name, d) in dependencies {
                if d.version.starts_with("link:") {
                    // TODO:
                    continue;
                }
                let mut is_aliased = false;
                let dep_pid = {
                    let mut dep_pid = None;
                    for (snap_key, snap_pid) in snap_pid.range(dep_name.as_ref()..) {
                        let Some(snap_rest) = snap_key.strip_prefix(dep_name.as_ref()) else {
                            break;
                        };
                        let Some(snap_rest) = snap_rest.strip_prefix("@") else {
                            continue;
                        };
                        if snap_rest == d.version {
                            dep_pid = Some(*snap_pid);
                            break;
                        }
                    }
                    if let Some(dep_pid) = dep_pid {
                        dep_pid
                    } else if let Some(dep_pid) = snap_pid.get(d.version.as_ref()) {
                        // If the dependency is aliased
                        is_aliased = true;
                        *dep_pid
                    } else {
                        return Err(Error::DependencyPackageNotFound(format!(
                            "{dep_name}@{}",
                            d.version
                        )));
                    }
                };
                let mut dep = DependencyBuilder::new(kind, importer_pid, dep_pid);
                if is_aliased {
                    dep.alias_name(PackageName::new(dep_name.to_string())?);
                }
                dep.svs(SourceVersionSpecifier::new(d.specifier.to_string())?);
                chastefile.add_dependency(dep.build());
            }
        }
    }
    for (pkg_desc, snap) in &lockfile.snapshots {
        let pkg_pid = *snap_pid.get(pkg_desc.as_ref()).unwrap();
        let (pkg_peers, _) = match pid_peers.get(&pkg_pid) {
            Some((pd, pm)) => (Some(pd), Some(pm)),
            None => (None, None),
        };
        for (dependencies, kind_) in [
            (&snap.dependencies, DependencyKind::Dependency),
            (
                &snap.optional_dependencies,
                DependencyKind::OptionalDependency,
            ),
        ] {
            for (dep_name, dep_svd) in dependencies {
                let (kind, svs) = if let Some(svs) =
                    pkg_peers.as_ref().and_then(|p| p.get(dep_name))
                {
                    match kind_ {
                        DependencyKind::Dependency => (DependencyKind::PeerDependency, Some(svs)),
                        DependencyKind::OptionalDependency => {
                            (DependencyKind::OptionalPeerDependency, Some(svs))
                        }
                        _ => unreachable!(),
                    }
                } else {
                    (kind_, None)
                };
                let mut dep = DependencyBuilder::new(kind, pkg_pid, {
                    let mut dep_pid = None;
                    for (snap_key, &snap_pid) in snap_pid.range(dep_name.as_ref()..) {
                        let Some(snap_rest) = snap_key.strip_prefix(dep_name.as_ref()) else {
                            break;
                        };
                        let Some(snap_rest) = snap_rest.strip_prefix("@") else {
                            continue;
                        };
                        if snap_rest == dep_svd {
                            dep_pid = Some(snap_pid);
                            break;
                        }
                    }
                    if let Some(dep_pid) = dep_pid {
                        dep_pid
                    } else if let Some(dep_pid) = snap_pid.get(dep_svd.as_ref()) {
                        // If the dependency is aliased
                        *dep_pid
                    } else {
                        return Err(Error::DependencyPackageNotFound(format!(
                            "{dep_name}@{dep_svd}"
                        )));
                    }
                });
                if let Some(svs) = svs {
                    dep.svs(SourceVersionSpecifier::new(svs.to_string())?);
                }
                chastefile.add_dependency(dep.build());
            }
        }
    }

    Ok(chastefile.build()?)
}
