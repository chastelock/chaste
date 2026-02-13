// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use chaste_types::{
    package_name_str, ssri, Chastefile, ChastefileBuilder, Checksums, DependencyBuilder,
    DependencyKind, InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageDerivation,
    PackageDerivationMetaBuilder, PackageID, PackageName, PackagePatchBuilder, PackageSource,
    PackageSourceType, PackageVersion, SourceVersionSpecifier, PACKAGE_JSON_FILENAME,
    ROOT_MODULE_PATH,
};

use itertools::Itertools as _;
use nom::bytes::complete::tag;
use nom::combinator::eof;
use nom::Parser as _;
use yarn_lock_parser as yarn;

use crate::berry::types::PackageJson;
use crate::btree_candidates::Candidates;
use crate::error::{Error, Result};
use crate::resolutions::{is_same_svs, Resolutions};
use crate::Meta;

mod mjam;
mod types;

fn parse_checksum(integrity: &str) -> Result<Checksums> {
    // In v8 lockfiles, there is a prefix like "10/".
    let integrity = integrity
        .split_once("/")
        .map(|(_, i)| i)
        .unwrap_or(integrity);
    Ok(Checksums::RepackZip(Integrity::from_hex(
        integrity,
        ssri::Algorithm::Sha512,
    )?))
}

fn parse_package(entry: &yarn::Entry) -> Result<(PackageBuilder, Option<PackageSource>)> {
    let source = mjam::parse_source(entry);
    let name = match &source {
        Some((n, _)) => n,
        _ => entry.name,
    };
    let mut pkg = PackageBuilder::new(
        Some(PackageName::new(name.to_string())?),
        Some(entry.version.to_string()),
    );
    if !entry.integrity.is_empty() {
        pkg.checksums(parse_checksum(entry.integrity)?);
    }
    let source = if let Some((_, Some(source))) = source {
        pkg.source(source.clone());
        Some(source)
    } else {
        None
    };
    Ok((pkg, source))
}

fn find_dep_pid<'a, S>(
    descriptor: &'a (S, S),
    from_entry: &yarn::Entry,
    resolutions: &Resolutions<'a>,
    descriptor_to_pid: &BTreeMap<(&'a str, &'a str), PackageID>,
) -> Result<Option<PackageID>>
where
    S: AsRef<str>,
{
    let (descriptor_name, descriptor_svs) = (descriptor.0.as_ref(), descriptor.1.as_ref());
    let overridden_resolution = resolutions.find((descriptor_name, descriptor_svs), || {
        &from_entry.descriptors
    });
    let evaluated_svs = overridden_resolution.unwrap_or(descriptor_svs);
    let mut candidate_entries = Candidates::new(descriptor_name, &descriptor_to_pid)
        .filter(|((_, d_s), _)| is_same_svs(evaluated_svs, d_s));
    if let Some((_, pid)) = candidate_entries.next() {
        if candidate_entries.next().is_some() {
            return Err(Error::AmbiguousResolution(format!(
                "{descriptor_name}@{descriptor_svs}",
            )));
        }
        return Ok(Some(*pid));
    }

    Err(Error::DependencyNotFound(format!(
        "{descriptor_name}@{descriptor_svs}",
    )))
}

fn find_peer_pid<'a, S>(
    descriptor: &'a (S, S),
    from_pid: PackageID,
    from_entry: &yarn::Entry,
    resolutions: &Resolutions<'a>,
    descriptor_to_pid: &BTreeMap<(&'a str, &'a str), PackageID>,
    pid_to_entry: &HashMap<PackageID, &'a yarn::Entry<'a>>,
    dep_children: &HashMap<PackageID, Vec<PackageID>>,
    package_sources: &HashMap<PackageID, PackageSource>,
) -> Result<Option<PackageID>>
where
    S: AsRef<str>,
{
    let (descriptor_name, descriptor_svs) = (descriptor.0.as_ref(), descriptor.1.as_ref());
    let overridden_resolution = resolutions.find((descriptor_name, descriptor_svs), || {
        &from_entry.descriptors
    });

    let candidate_entries =
        Candidates::new(descriptor_name, &descriptor_to_pid).collect::<Vec<_>>();

    // If there's just one candidate to consider, it's easy.
    if let [(_, pid)] = *candidate_entries {
        return Ok(Some(*pid));
    }
    // Peer dependencies can be optional or unfulfilled.
    if candidate_entries.len() == 0 {
        return Ok(None);
    }
    // If an SVS is overridden through package.json "resolutions" field,
    // it takes that field's value into its descriptors.
    if let Some(evaluated_svs) = overridden_resolution {
        if let [(_, pid)] = *candidate_entries
            .iter()
            .filter(|((_, d_s), _)| is_same_svs(evaluated_svs, d_s))
            .collect::<Vec<_>>()
        {
            return Ok(Some(**pid));
        }
    }

    // This is where fun begins.

    // Check if the dependent package's other dependencies match.
    // (Sometimes the package may have the same dependency as both regular and peer.)
    let siblings_packages: HashSet<PackageID> = HashSet::from_iter(
        dep_children
            .get(&from_pid)
            .unwrap_or(&vec![])
            .iter()
            .copied(),
    );
    let candidate_siblings = siblings_packages
        .iter()
        .filter(|sib| candidate_entries.iter().any(|(_, cand)| sib == cand))
        .collect::<Vec<_>>();
    if let [pid] = *candidate_siblings {
        return Ok(Some(*pid));
    }

    // If a package is requested somewhere else with the same specifier.
    let same_spec_candidates = candidate_entries
        .iter()
        .filter(|((_, s), _)| {
            *s == descriptor_svs || s.strip_prefix("npm:") == Some(descriptor_svs)
        })
        .collect::<Vec<_>>();
    if let [(_, pid)] = *same_spec_candidates {
        return Ok(Some(**pid));
    }

    // If the dependency is back on the package that requested it.
    if candidate_entries
        .iter()
        .find(|(_, pid)| **pid == from_pid)
        .is_some()
    {
        return Ok(Some(from_pid));
    }

    // Finally, check which candidates satisfy the version requirement.
    let svs = SourceVersionSpecifier::new(descriptor_svs.to_owned())?;
    if let Some(range) = svs.npm_range() {
        let mut satisfying_candidates = candidate_entries
            .iter()
            .filter_map(|(_, pid)| {
                if package_sources
                    .get(pid)
                    .is_some_and(|s| s.source_type() == PackageSourceType::Npm)
                {
                    let entry = pid_to_entry.get(*pid).unwrap();
                    Some((pid, PackageVersion::parse(entry.version).unwrap()))
                        .filter(|(_, v)| v.satisfies(&range))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if let [(pid, _)] = *satisfying_candidates {
            return Ok(Some(**pid));
        }
        // If there are more than 2 matching candidates, choose the one with higher version.
        if satisfying_candidates.len() > 1 {
            satisfying_candidates.sort_by(|a, b| a.1.cmp(&b.1));
            return Ok(Some(**satisfying_candidates.last().unwrap().0));
        }
    }

    // A dependency may be unmet. That's ok. In fact, upstream allows that. Everyone ignores the
    // "@chastelock/testcase@workspace:. doesn't provide acorn (pf1eb59), requested by @sveltejs/acorn-typescript."
    Ok(None)
}

pub(crate) fn resolve<'y, FG>(
    yarn_lock: yarn::Lockfile<'y>,
    root_dir: &Path,
    file_getter: &FG,
) -> Result<Chastefile<Meta>>
where
    FG: Fn(PathBuf) -> Result<String, io::Error>,
{
    let root_package_contents = file_getter(root_dir.join(PACKAGE_JSON_FILENAME))?;
    let root_package_json: PackageJson = serde_json::from_str(&root_package_contents)?;

    let mut resolutions = Resolutions::new();
    for (rk, rv) in &root_package_json.resolutions {
        resolutions.insert(rk, rv.as_ref())?;
    }

    let mut chastefile_builder = ChastefileBuilder::new(Meta {
        lockfile_version: yarn_lock.version,
    });
    let mut descriptor_to_pid: BTreeMap<(&'y str, &'y str), PackageID> = BTreeMap::new();
    let mut pid_to_entry: HashMap<PackageID, &yarn::Entry> =
        HashMap::with_capacity(yarn_lock.entries.len());
    let mut package_sources: HashMap<PackageID, PackageSource> =
        HashMap::with_capacity(yarn_lock.entries.len());

    let mut deferred_pkgs = Vec::new();

    {
        for entry in &yarn_lock.entries {
            let (pkg, source) = parse_package(entry)?;

            // For patch: packages, we need to mark the derivation, for which
            // we need the PackageID they're derived from.
            if (package_name_str, tag("@patch:"))
                .parse(entry.resolved)
                .is_ok()
            {
                deferred_pkgs.push((entry, pkg));
                continue;
            }

            let pid = chastefile_builder.add_package(pkg.build()?)?;
            pid_to_entry.insert(pid, entry);
            for (pn, ps) in &entry.descriptors {
                if descriptor_to_pid.insert((pn, ps), pid).is_some() {
                    return Err(Error::DuplicateSpecifiers(format!("{pn}@{ps}")));
                }
            }
            if let Some(src) = source {
                package_sources.insert(pid, src);
            }
            if let Some(workspace_path) = entry
                .descriptors
                .iter()
                .find_map(|(_, e_svs)| e_svs.strip_prefix("workspace:"))
            {
                if workspace_path == "." {
                    chastefile_builder.set_root_package_id(pid)?;
                    let installation_builder =
                        InstallationBuilder::new(pid, ROOT_MODULE_PATH.clone());
                    chastefile_builder.add_package_installation(installation_builder.build()?);
                } else {
                    chastefile_builder.set_as_workspace_member(pid)?;
                    chastefile_builder.add_package_installation(
                        InstallationBuilder::new(pid, ModulePath::new(workspace_path.to_string())?)
                            .build()?,
                    );
                }
            }
        }
    }
    for (entry, mut pkg) in deferred_pkgs {
        let Some((_, patched_pkg_name, patched_pkg_svd_penc, patch_path, _patch_meta)) =
            mjam::patch_descriptor(entry.resolved)
        else {
            return Err(Error::InvalidResolved(entry.resolved.to_string()));
        };
        // "npm%3A0.1.0" -> "npm:0.1.0"
        let patched_pkg_svd =
            percent_encoding::percent_decode_str(patched_pkg_svd_penc).decode_utf8()?;
        let mut source_candidates = Candidates::new(patched_pkg_name, &descriptor_to_pid)
            .unique_by(|(_, cpid)| **cpid)
            .filter(|(_, cpid)| {
                (
                    tag::<&str, &str, ()>(patched_pkg_name),
                    tag("@"),
                    tag(patched_pkg_svd.as_ref()),
                    eof,
                )
                    .parse(pid_to_entry.get(cpid).unwrap().resolved)
                    .is_ok()
            });
        let Some((_, &source_pid)) = source_candidates.next() else {
            return Err(Error::InvalidResolved(entry.resolved.to_string()));
        };
        if source_candidates.next().is_some() {
            return Err(Error::AmbiguousResolved(entry.resolved.to_string()));
        }
        let patch = PackagePatchBuilder::new(
            patch_path
                .strip_prefix("./")
                .unwrap_or(patch_path)
                .to_string(),
        );
        pkg.derived(
            PackageDerivationMetaBuilder::new(PackageDerivation::Patch(patch.build()?), source_pid)
                .build()?,
        );
        let pid = chastefile_builder.add_package(pkg.build()?)?;
        pid_to_entry.insert(pid, entry);
        for (pn, ps) in &entry.descriptors {
            if descriptor_to_pid.insert((pn, ps), pid).is_some() {
                return Err(Error::DuplicateSpecifiers(format!("{pn}@{ps}")));
            }
        }
    }

    let maybe_state_contents =
        match file_getter(root_dir.join("node_modules").join(".yarn-state.yml")) {
            Ok(s) => Some(s),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
    let maybe_state = maybe_state_contents
        .as_ref()
        .map(|sc| yarn_state::parse(sc))
        .transpose()?;
    if let Some(state) = maybe_state {
        for st8_pkg in &state.packages {
            let expected_resolution = mjam::resolution_from_state_key(st8_pkg.resolution);
            let (&pid, _) = pid_to_entry
                .iter()
                .find(|(_, e)| e.resolved == expected_resolution)
                .ok_or_else(|| Error::StatePackageNotFound(expected_resolution.to_string()))?;
            for st8_location in &st8_pkg.locations {
                let installation =
                    InstallationBuilder::new(pid, ModulePath::new(st8_location.to_string())?)
                        .build()?;
                chastefile_builder.add_package_installation(installation);
            }
        }
    }

    let mut dep_children: HashMap<PackageID, Vec<PackageID>> = HashMap::new();

    for (from_pid, entry) in pid_to_entry.iter() {
        // In berry, dependencies are stored in 2 sections under an Entry:
        // either "peerDependencies:", "dependencies:".
        // If they are optional, this will be indicated in "peerDependenciesMeta:",
        // same as in a package.json, or, "dependenciesMeta:", presumably an analogy to that.

        // In classic, they were stored in an "optionalDependencies:" section,
        // but, as per above, this shouldn't be a thing here.
        debug_assert!(entry.optional_dependencies.is_empty());

        let mut deps_meta = HashMap::with_capacity(entry.dependencies_meta.len());
        for (k, v) in &entry.dependencies_meta {
            deps_meta.insert(k, v);
        }
        for dep_descriptor in &entry.dependencies {
            let kind = match deps_meta.get(&dep_descriptor.0).and_then(|m| m.optional) {
                Some(true) => DependencyKind::OptionalDependency,
                Some(false) | None => DependencyKind::Dependency,
            };
            let Some(dep_pid) =
                find_dep_pid(dep_descriptor, entry, &resolutions, &descriptor_to_pid)?
            else {
                continue;
            };
            let mut dep = DependencyBuilder::new(kind, *from_pid, dep_pid);
            dep_children
                .get_mut(from_pid)
                .map(|l| l.push(dep_pid))
                .unwrap_or_else(|| {
                    dep_children.insert(*from_pid, vec![dep_pid]);
                });
            let svs = SourceVersionSpecifier::new(dep_descriptor.1.to_string())?;
            if svs.aliased_package_name().is_some() {
                dep.alias_name(PackageName::new(dep_descriptor.0.to_string())?);
            }
            dep.svs(svs);
            chastefile_builder.add_dependency(dep.build());
        }
    }
    for (from_pid, entry) in pid_to_entry.iter() {
        let mut deps_meta = HashMap::with_capacity(entry.peer_dependencies_meta.len());
        for (k, v) in &entry.peer_dependencies_meta {
            deps_meta.insert(k, v);
        }
        for dep_descriptor in &entry.peer_dependencies {
            let kind = match deps_meta.get(&dep_descriptor.0).and_then(|m| m.optional) {
                Some(true) => DependencyKind::OptionalPeerDependency,
                Some(false) | None => DependencyKind::PeerDependency,
            };
            let Some(dep_pid) = find_peer_pid(
                dep_descriptor,
                *from_pid,
                entry,
                &resolutions,
                &descriptor_to_pid,
                &pid_to_entry,
                &dep_children,
                &package_sources,
            )?
            else {
                continue;
            };
            dep_children
                .get_mut(from_pid)
                .map(|l| l.push(dep_pid))
                .unwrap_or_else(|| {
                    dep_children.insert(*from_pid, vec![dep_pid]);
                });
            let mut dep = DependencyBuilder::new(kind, *from_pid, dep_pid);
            let svs = SourceVersionSpecifier::new(dep_descriptor.1.to_string())?;
            if svs.aliased_package_name().is_some() {
                dep.alias_name(PackageName::new(dep_descriptor.0.to_string())?);
            }
            dep.svs(svs);
            chastefile_builder.add_dependency(dep.build());
        }
    }
    Ok(chastefile_builder.build()?)
}
