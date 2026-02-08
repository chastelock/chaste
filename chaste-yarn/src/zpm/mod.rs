// SPDX-FileCopyrightText: 2026 The Chaste Authors
// SPDX-License-Identifier: BSD-2-Clause

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use chaste_types::{
    ssri, Chastefile, ChastefileBuilder, Checksums, Dependency, DependencyBuilder, DependencyKind,
    InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageID, PackageName,
    SourceVersionSpecifier, ROOT_MODULE_PATH,
};

use crate::btree_candidates::Candidates;
use crate::error::Result;
use crate::resolutions::{is_same_svs_zpm, Resolutions};
use crate::{Error, Meta};

mod mjam;
mod types;

const PACKAGE_JSON_FILENAME: &str = "package.json";

pub(crate) fn resolve<'y, FG>(
    lockfile_contents: &'y str,
    root_dir: &Path,
    file_getter: &FG,
) -> Result<Chastefile<Meta>>
where
    FG: Fn(PathBuf) -> Result<String, io::Error>,
{
    let lockfile: types::Lockfile<'y> = serde_json::from_str(lockfile_contents)?;
    if lockfile.metadata.version != 9 {
        return Err(Error::UnknownLockfileVersion(lockfile.metadata.version));
    }

    let root_package_contents = file_getter(root_dir.join(PACKAGE_JSON_FILENAME))?;
    let root_package_json: types::PackageJson = serde_json::from_str(&root_package_contents)?;

    let mut resolutions = Resolutions::new();
    for (key, value) in &root_package_json.resolutions {
        resolutions.insert(key, value)?;
    }

    let mut chastefile = ChastefileBuilder::new(Meta {
        lockfile_version: lockfile.metadata.version,
    });

    let root_pid = chastefile.add_package(
        PackageBuilder::new(
            root_package_json
                .name
                .map(|n| PackageName::new(n.to_string()))
                .transpose()?,
            root_package_json.version.map(|v| v.to_string()),
        )
        .build()?,
    )?;
    chastefile.set_root_package_id(root_pid)?;
    chastefile.add_package_installation(
        InstallationBuilder::new(root_pid, ROOT_MODULE_PATH.clone()).build()?,
    );

    let mut spec_to_pid: BTreeMap<(&'y str, &'y str), PackageID> = BTreeMap::new();
    let mut ekey_to_pid: BTreeMap<&'y str, PackageID> = BTreeMap::new();
    for (key, entry) in lockfile.entries.iter() {
        let specifiers = mjam::specifiers(key)?;
        let (name, resolved) = mjam::resolved(entry.resolution.resolution)?;
        let mut pkg = PackageBuilder::new(
            Some(PackageName::new(name.to_owned())?),
            Some(entry.resolution.version.to_owned()),
        );
        let mut workspace_path = None;
        match resolved {
            mjam::Resolved::Remote(src) => pkg.source(src),
            mjam::Resolved::Workspace(path) => {
                workspace_path = Some(path);
            }
        }
        if let Some(checksum) = entry.checksum {
            pkg.checksums(Checksums::RepackZip(Integrity::from_hex(
                checksum,
                ssri::Sha512,
            )?));
        }
        let pid = chastefile.add_package(pkg.build()?)?;
        if let Some(path) = workspace_path {
            chastefile.set_as_workspace_member(pid)?;
            chastefile.add_package_installation(
                InstallationBuilder::new(pid, ModulePath::new(path.to_owned())?).build()?,
            );
        }
        for spec in specifiers {
            if spec_to_pid.insert(spec, pid).is_some() {
                return Err(Error::DuplicateSpecifiers(format!("{}@{}", spec.0, spec.1)));
            }
        }
        ekey_to_pid.insert(key, pid);
    }

    for (kind, dependencies) in [
        (DependencyKind::Dependency, &root_package_json.dependencies),
        (
            DependencyKind::DevDependency,
            &root_package_json.dev_dependencies,
        ),
        (
            DependencyKind::OptionalDependency,
            &root_package_json.optional_dependencies,
        ),
        (
            DependencyKind::PeerDependency,
            &root_package_json.peer_dependencies,
        ),
    ] {
        for (dep_name, dep_svs) in dependencies {
            if let Some(dep) = resolve_dependency(
                (dep_name, dep_svs),
                kind,
                &resolutions,
                root_pid,
                &[],
                &spec_to_pid,
            )? {
                chastefile.add_dependency(dep);
            }
        }
    }

    for kind_ in [DependencyKind::Dependency, DependencyKind::PeerDependency] {
        for (key, entry) in lockfile.entries.iter() {
            let (dependencies, optional_deps) = match kind_ {
                DependencyKind::Dependency => (
                    &entry.resolution.dependencies,
                    &entry.resolution.optional_dependencies,
                ),
                DependencyKind::PeerDependency => (
                    &entry.resolution.peer_dependencies,
                    &entry.resolution.optional_peer_dependencies,
                ),
                _ => unreachable!(),
            };
            let &from_pid = ekey_to_pid.get(key).unwrap();
            // XXX: this is pointlessly run even if there are no relevant resolutions
            let parent_specifiers = mjam::specifiers(key)?;
            for (dep_name, dep_svs) in dependencies {
                let kind = match (kind_, optional_deps.contains(dep_name)) {
                    (DependencyKind::Dependency, false) => DependencyKind::Dependency,
                    (DependencyKind::Dependency, true) => DependencyKind::OptionalDependency,
                    (DependencyKind::PeerDependency, false) => DependencyKind::PeerDependency,
                    (DependencyKind::PeerDependency, true) => {
                        DependencyKind::OptionalPeerDependency
                    }
                    _ => unreachable!(),
                };
                if let Some(dep) = resolve_dependency(
                    (dep_name, dep_svs),
                    kind,
                    &resolutions,
                    from_pid,
                    &parent_specifiers,
                    &spec_to_pid,
                )? {
                    chastefile.add_dependency(dep);
                }
            }
        }
    }

    chastefile.build().map_err(Error::ChasteError)
}

fn resolve_dependency(
    (dep_name, dep_svs): (&str, &str),
    kind: DependencyKind,
    resolutions: &Resolutions,
    from_pid: PackageID,
    parent_specifiers: &[(&str, &str)],
    spec_to_pid: &BTreeMap<(&str, &str), PackageID>,
) -> Result<Option<Dependency>> {
    let override_spec = resolutions.find((dep_name, dep_svs), || parent_specifiers);
    let evaluated_spec = override_spec.unwrap_or(dep_svs);
    let original_svs = SourceVersionSpecifier::new(dep_svs.to_string())?;
    let override_svs = if let Some(ospec) = override_spec {
        &SourceVersionSpecifier::new(ospec.to_string())?
    } else {
        &original_svs
    };
    let alias = override_svs.aliased_package_name();
    let alias_spec = override_svs
        .npm_range_str()
        .filter(|_| alias.is_some())
        .unwrap_or(evaluated_spec);
    let candidates = Candidates::new(
        alias.as_ref().map(|n| n.as_ref()).unwrap_or(dep_name),
        &spec_to_pid,
    );
    let Some(pid) = (if kind.is_peer() {
        resolve_peer_dependency(candidates.collect())?
    } else {
        let mut candidates = candidates.filter(|((_, s), _)| is_same_svs_zpm(alias_spec, s));
        let Some((_, pid)) = candidates.next() else {
            if kind.is_optional() {
                return Ok(None);
            }
            return Err(Error::DependencyNotFound(format!("{dep_name}@{dep_svs}")));
        };
        if candidates.next().is_some() {
            return Err(Error::AmbiguousResolution(format!("{dep_name}@{dep_svs}")));
        }
        Some(*pid)
    }) else {
        return Ok(None);
    };
    let mut dep = DependencyBuilder::new(kind, from_pid, pid);
    if original_svs.aliased_package_name().is_some() {
        dep.alias_name(PackageName::new(dep_name.to_string())?);
    }
    dep.svs(original_svs);
    Ok(Some(dep.build()))
}

fn resolve_peer_dependency<'y>(
    candidate_entries: Vec<(&(&'y str, &'y str), &PackageID)>,
) -> Result<Option<PackageID>> {
    // If there's just one candidate to consider, it's easy.
    if let [(_, pid)] = *candidate_entries {
        return Ok(Some(*pid));
    }
    // Peer dependencies can be optional.
    // TODO: also check peerDependenciesMeta
    if candidate_entries.len() == 0 {
        return Ok(None);
    }

    todo!();
}
