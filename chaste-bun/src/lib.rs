// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use chaste_types::{
    package_name_part, Chastefile, ChastefileBuilder, Checksums, DependencyBuilder, DependencyKind,
    InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageID, PackageName,
    PackageSource, SourceVersionSpecifier, SourceVersionSpecifierKind,
};
use nom::{
    bytes::complete::tag,
    combinator::{eof, map, opt, recognize, rest, verify},
    multi::many0,
    sequence::{preceded, terminated},
    IResult, Parser,
};

pub use crate::error::{Error, Result};
use crate::types::{BunLock, LockPackageElement};

mod error;
#[cfg(test)]
mod tests;
mod types;

pub static LOCKFILE_NAME: &str = "bun.lock";

fn package_name(input: &str) -> IResult<&str, &str, nom::error::Error<&str>> {
    recognize((
        opt(preceded(tag("@"), terminated(package_name_part, tag("/")))),
        verify(package_name_part, |part: &str| {
            part != "node_modules" && part != "favicon.ico"
        }),
    ))
    .parse(input)
}

type SourceKey<'a> = (&'a str, Vec<&'a str>);

fn parse_package_key(input: &str) -> Result<(Option<SourceKey>, &str)> {
    (
        map(many0(terminated(package_name, tag("/"))), |pns| {
            if !pns.is_empty() {
                Some((
                    &input[..pns.iter().fold(0, |acc, pn| acc + pn.len() + 1) - 1],
                    pns,
                ))
            } else {
                None
            }
        }),
        terminated(package_name, eof),
    )
        .parse(input)
        .map(|(_, r)| r)
        .map_err(|_| Error::InvalidKey(input.to_string()))
}

fn parse_descriptor(input: &str) -> Result<(&str, &str)> {
    (terminated(package_name, tag("@")), rest)
        .parse(input)
        .map(|(_, r)| r)
        .map_err(|_| Error::InvalidKey(input.to_string()))
}

pub fn parse<P>(root_dir: P) -> Result<Chastefile>
where
    P: AsRef<Path>,
{
    let bun_lock_contents = fs::read_to_string(root_dir.as_ref().join(LOCKFILE_NAME))?;
    let bun_lock: BunLock = json5::from_str(&bun_lock_contents)?;

    if !matches!(bun_lock.lockfile_version, (0..=1)) {
        return Err(Error::UnknownLockfileVersion(bun_lock.lockfile_version));
    }

    let mut chastefile = ChastefileBuilder::new();

    let mut ws_location_to_pid: HashMap<&str, PackageID> =
        HashMap::with_capacity(bun_lock.workspaces.len());
    for (ws_location, ws_member) in &bun_lock.workspaces {
        let ws_path = ModulePath::new(ws_location.to_string())?;
        let pkg_builder = PackageBuilder::new(
            ws_member
                .name
                .as_ref()
                .map(|n| PackageName::new(n.to_string()))
                .transpose()?,
            ws_member.version.as_ref().map(|n| n.to_string()),
        );
        let pid = chastefile.add_package(pkg_builder.build()?)?;
        chastefile.add_package_installation(InstallationBuilder::new(pid, ws_path).build()?);
        if ws_location.is_empty() {
            chastefile.set_root_package_id(pid)?;
        } else {
            chastefile.set_as_workspace_member(pid)?;
        }
        ws_location_to_pid.insert(ws_location, pid);
    }

    let mut descript_to_pid: HashMap<&str, PackageID> =
        HashMap::with_capacity(bun_lock.packages.len());
    let mut presolved_unhoistable: HashMap<(&str, &str), PackageID> = HashMap::new();
    let mut aliased_pids: HashSet<PackageID> = HashSet::new();
    for (lock_key, lock_pkg) in &bun_lock.packages {
        let (source, installation_package_name) = parse_package_key(lock_key)?;
        let descriptor = match &lock_pkg[..] {
            [LockPackageElement::String(d), ..] => d.as_ref(),
            _ => return Err(Error::InvalidVariant(lock_key.to_string())),
        };
        // Packages repeat, so we dedup them by the descriptor.
        // But we still want to reverse search them by key.
        if let Some(pid) = descript_to_pid.get(descriptor) {
            if let Some((source_key, _)) = source {
                presolved_unhoistable.insert((source_key, installation_package_name), *pid);
            }
        } else {
            let (package_name, sv_marker) = parse_descriptor(descriptor)?;
            let pid = if let Some(pid) = sv_marker
                .strip_prefix("workspace:")
                .and_then(|l| ws_location_to_pid.get(l))
            {
                *pid
            } else {
                let sm_svs = SourceVersionSpecifier::new(sv_marker.to_string())?;
                let mut pkg_builder =
                    PackageBuilder::new(Some(PackageName::new(package_name.to_string())?), None);
                match (&lock_pkg[..], sm_svs.kind()) {
                    (
                        [LockPackageElement::String(_descriptor), LockPackageElement::String(_registry_url), LockPackageElement::Relations(_relations), LockPackageElement::String(integrity)],
                        SourceVersionSpecifierKind::Npm,
                    ) => {
                        pkg_builder.version(Some(sv_marker.to_string()));
                        let integrity = integrity.parse::<Integrity>()?;
                        if !integrity.hashes.is_empty() {
                            pkg_builder.checksums(Checksums::Tarball(integrity));
                        }
                        pkg_builder.source(PackageSource::Npm);
                    }
                    (_, SourceVersionSpecifierKind::TarballURL) => {
                        pkg_builder.source(PackageSource::TarballURL {
                            url: sv_marker.to_string(),
                        });
                    }
                    (_, SourceVersionSpecifierKind::Git | SourceVersionSpecifierKind::GitHub) => {
                        if !sm_svs.is_github() {
                            pkg_builder.source(PackageSource::Git {
                                url: sv_marker.to_string(),
                            });
                        }
                    }
                    (_, _) => return Err(Error::VariantMarkerMismatch(lock_key.to_string())),
                }
                let p = chastefile.add_package(pkg_builder.build()?)?;
                if installation_package_name != package_name {
                    aliased_pids.insert(p);
                }
                p
            };
            descript_to_pid.insert(descriptor, pid);
            if let Some((source_key, _)) = source {
                presolved_unhoistable.insert((source_key, installation_package_name), pid);
            }
            let module_path = ModulePath::new(if let Some((_, parent_modules)) = source {
                let expected_len = lock_key.len() + (parent_modules.len() * 13) + 13;
                let mut mp = String::with_capacity(expected_len);
                for pm in parent_modules {
                    mp += "node_modules/";
                    mp += pm;
                    mp += "/";
                }
                mp += "node_modules/";
                mp += installation_package_name;
                debug_assert_eq!(mp.len(), expected_len);
                mp
            } else {
                format!("node_modules/{installation_package_name}")
            })?;
            chastefile
                .add_package_installation(InstallationBuilder::new(pid, module_path).build()?);
        }
    }
    for (lock_key, lock_pkg) in &bun_lock.packages {
        let descriptor = match &lock_pkg[..] {
            [LockPackageElement::String(d), ..] => d.as_ref(),
            // This should have thrown an InvalidVariant earlier
            _ => unreachable!(),
        };
        let relations = match &lock_pkg[..] {
            [_, LockPackageElement::Relations(r), ..] => Some(r),
            [_, _, LockPackageElement::Relations(r), ..] => Some(r),
            _ => None,
        };
        debug_assert!(
            relations.is_some()
                || !lock_pkg
                    .iter()
                    .any(|p| matches!(p, LockPackageElement::Relations(_)))
        );
        let pid = *descript_to_pid.get(descriptor).unwrap();
        if let Some(relations) = relations {
            for (deps, kind_) in [
                (&relations.dependencies, DependencyKind::Dependency),
                (&relations.dev_dependencies, DependencyKind::DevDependency),
                (&relations.peer_dependencies, DependencyKind::PeerDependency),
                (
                    &relations.optional_dependencies,
                    DependencyKind::OptionalDependency,
                ),
            ] {
                for (dep_name, dep_svs) in deps {
                    let kind = match kind_ {
                        DependencyKind::PeerDependency
                            if relations.optional_peers.contains(dep_name) =>
                        {
                            DependencyKind::OptionalPeerDependency
                        }
                        k => k,
                    };
                    match presolved_unhoistable
                        .get(&(lock_key, dep_name))
                        .or_else(|| {
                            bun_lock.packages.get(dep_name).and_then(|p| {
                                let descriptor = match &p[..] {
                                    [LockPackageElement::String(d), ..] => d.as_ref(),
                                    // This should have thrown InvalidVariant earlier
                                    _ => unreachable!(),
                                };
                                descript_to_pid.get(descriptor)
                            })
                        }) {
                        Some(dep_pid) => {
                            let mut dep = DependencyBuilder::new(kind, pid, *dep_pid);
                            dep.svs(SourceVersionSpecifier::new(dep_svs.to_string())?);
                            chastefile.add_dependency(dep.build());
                        }
                        None if kind.is_optional() => {}
                        None => {
                            return Err(Error::DependencyNotFound(format!("{dep_name}@{dep_svs}")))
                        }
                    };
                }
            }
        }
    }
    for (ws_location, ws_member) in &bun_lock.workspaces {
        let relations = &ws_member.relations;
        let pid = *ws_location_to_pid.get(ws_location.as_ref()).unwrap();

        for (deps, kind_) in [
            (&relations.dependencies, DependencyKind::Dependency),
            (&relations.dev_dependencies, DependencyKind::DevDependency),
            (&relations.peer_dependencies, DependencyKind::PeerDependency),
            (
                &relations.optional_dependencies,
                DependencyKind::OptionalDependency,
            ),
        ] {
            for (dep_name, dep_svs) in deps {
                let kind = match kind_ {
                    DependencyKind::PeerDependency
                        if relations.optional_peers.contains(dep_name) =>
                    {
                        DependencyKind::OptionalPeerDependency
                    }
                    k => k,
                };
                match bun_lock.packages.get(dep_name).and_then(|p| {
                    let descriptor = match &p[..] {
                        [LockPackageElement::String(d), ..] => d.as_ref(),
                        // This should have thrown InvalidVariant earlier
                        _ => unreachable!(),
                    };
                    descript_to_pid.get(descriptor)
                }) {
                    Some(dep_pid) => {
                        let mut dep = DependencyBuilder::new(kind, pid, *dep_pid);
                        dep.svs(SourceVersionSpecifier::new(dep_svs.to_string())?);
                        if aliased_pids.contains(dep_pid) {
                            dep.alias_name(PackageName::new(dep_name.to_string())?);
                        }
                        chastefile.add_dependency(dep.build());
                    }
                    None if kind.is_optional() => {}
                    None => return Err(Error::DependencyNotFound(format!("{dep_name}@{dep_svs}"))),
                };
            }
        }
    }

    chastefile.build().map_err(Error::ChasteError)
}

#[cfg(test)]
mod unit_tests {
    use crate::{parse_package_key, Result};

    #[test]
    fn test_parse_package_key() -> Result<()> {
        assert_eq!(parse_package_key("ltx")?, (None, "ltx"));
        assert_eq!(parse_package_key("@types/node")?, (None, "@types/node"));
        assert_eq!(
            parse_package_key("@xmpp/xml/ltx")?,
            (Some(("@xmpp/xml", vec!["@xmpp/xml"])), "ltx")
        );
        assert_eq!(
            parse_package_key("socket.io/debug/ms")?,
            (Some(("socket.io/debug", vec!["socket.io", "debug"])), "ms")
        );
        Ok(())
    }
}
