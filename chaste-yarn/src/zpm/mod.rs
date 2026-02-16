// SPDX-FileCopyrightText: 2026 The Chaste Authors
// SPDX-License-Identifier: BSD-2-Clause

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use chaste_types::{
    ssri, Chastefile, ChastefileBuilder, Checksums, Dependency, DependencyBuilder, DependencyKind,
    InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageDerivation,
    PackageDerivationMetaBuilder, PackageID, PackageName, PackagePatchBuilder, PackageSource,
    PackageSourceType, PackageVersion, SourceVersionSpecifier, ROOT_MODULE_PATH,
};

use globreeks::Globreeks;
use itertools::Itertools as _;
use walkdir::WalkDir;
use yoke::Yoke;

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
    let mut package_builder_copy: HashMap<(&str, &str), PackageBuilder> = HashMap::new();
    let mut patched_packages: HashMap<(Cow<'_, str>, Cow<'_, str>), (&str, &str)> = HashMap::new();
    for (key, value) in &root_package_json.resolutions {
        resolutions.insert(key, value)?;

        if let Some((patched_spec, patch_path)) = mjam::patched_spec(value) {
            let Ok((_, (spec_pn, spec_sv))) = mjam::specifier(&patched_spec) else {
                // Ok should be guaranteed if we got Some earlier, by how mjam::patched_spec is done
                unreachable!();
            };
            patched_packages.insert(
                (spec_pn.to_string().into(), spec_sv.to_string().into()),
                (patch_path, value),
            );
        }
    }

    let mut chastefile = ChastefileBuilder::new(Meta {
        lockfile_version: lockfile.metadata.version,
    });

    let mut member_package_jsons: Vec<(String, Yoke<types::PackageJson, String>)> = Vec::new();
    let mut mpj_idx_to_pid: HashMap<usize, PackageID> = HashMap::new();
    if let Some(workspaces) = &root_package_json.workspaces {
        let dir_globset = Globreeks::new(workspaces.iter())?;

        for de_result in WalkDir::new(root_dir).into_iter() {
            let de = de_result?;
            if !de.file_type().is_file() || de.file_name() != PACKAGE_JSON_FILENAME {
                continue;
            }
            let absolute_path = de.path();
            let relative_workspace_path = absolute_path
                .parent()
                .unwrap()
                .strip_prefix(root_dir)
                .unwrap()
                .to_str()
                .expect("Path is Unicode-representable");
            if !dir_globset.evaluate(relative_workspace_path) {
                continue;
            }

            let absolute_pathbuf = absolute_path.to_path_buf();
            let member_package_json_contents = file_getter(absolute_pathbuf.clone())
                .map_err(|e| Error::IoInWorkspace(e, absolute_pathbuf))?;
            member_package_jsons.push((
                // must be owned because its lifetime goes out of scope
                relative_workspace_path.to_owned(),
                Yoke::try_attach_to_cart(member_package_json_contents, |cont| {
                    serde_json::from_str(cont)
                })?,
            ));
        }
    }

    let root_pid = chastefile.add_package(
        PackageBuilder::new(
            root_package_json
                .name
                .as_deref()
                .map(|n| PackageName::new(n.to_string()))
                .transpose()?,
            root_package_json.version.as_deref().map(|v| v.to_string()),
        )
        .build()?,
    )?;
    chastefile.set_root_package_id(root_pid)?;
    chastefile.add_package_installation(
        InstallationBuilder::new(root_pid, ROOT_MODULE_PATH.clone()).build()?,
    );

    let mut spec_to_pid: BTreeMap<(&str, &str), PackageID> = BTreeMap::new();
    if let Some(pn) = &root_package_json.name {
        spec_to_pid.insert((pn, ""), root_pid);
    }

    for (idx, (workspace_path, package_json)) in member_package_jsons
        .iter()
        .map(|(path, y)| (path, y.get()))
        .enumerate()
    {
        let pid = chastefile.add_package(
            PackageBuilder::new(
                package_json
                    .name
                    .as_deref()
                    .map(|n| PackageName::new(n.to_string()))
                    .transpose()?,
                package_json.version.as_deref().map(|v| v.to_string()),
            )
            .build()?,
        )?;
        if let Some(pn) = &package_json.name {
            spec_to_pid.insert((pn, ""), pid);
        }
        mpj_idx_to_pid.insert(idx, pid);
        chastefile.set_as_workspace_member(pid)?;
        chastefile.add_package_installation(
            InstallationBuilder::new(pid, ModulePath::new(workspace_path.clone())?).build()?,
        );
    }

    let mut ekey_to_pid: BTreeMap<&'y str, PackageID> = BTreeMap::new();
    let mut pid_to_entry: HashMap<PackageID, &types::Entry<'_>> =
        HashMap::with_capacity(lockfile.entries.len());
    let mut package_sources: HashMap<PackageID, PackageSource> =
        HashMap::with_capacity(lockfile.entries.len());
    for (key, entry) in lockfile.entries.iter() {
        let specifiers = mjam::specifiers(key)?;
        let (name, resolved) = mjam::resolved(entry.resolution.resolution)?;
        match resolved {
            Some(mjam::Resolved::Workspace(path)) => {
                let Some(pid) = member_package_jsons
                    .iter()
                    .enumerate()
                    .find(|(_, (p, _))| p == path)
                    .map(|(idx, _)| *mpj_idx_to_pid.get(&idx).unwrap())
                else {
                    return Err(Error::UnrecognizedWorkspaceMember(path.to_string()));
                };
                for spec in specifiers {
                    if spec_to_pid.insert(spec, pid).is_some() {
                        return Err(Error::DuplicateSpecifiers(format!("{}@{}", spec.0, spec.1)));
                    }
                }
                ekey_to_pid.insert(key, pid);
                pid_to_entry.insert(pid, entry);
                continue;
            }
            _ => {}
        }
        let mut pkg = PackageBuilder::new(
            Some(PackageName::new(name.to_owned())?),
            Some(entry.resolution.version.to_owned()),
        );
        match resolved {
            Some(mjam::Resolved::Remote(ref src)) => {
                pkg.source(src.clone());
            }
            None => {}
            Some(mjam::Resolved::Workspace(_)) => unreachable!(),
        }
        if let Some(checksum) = entry.checksum {
            pkg.checksums(Checksums::RepackZip(Integrity::from_hex(
                checksum,
                ssri::Sha512,
            )?));
        }
        for spec in &specifiers {
            if patched_packages.contains_key(&(Cow::Borrowed(spec.0), Cow::Borrowed(spec.1))) {
                package_builder_copy.insert((spec.0, spec.1), pkg.clone());
            }
        }
        let pid = chastefile.add_package(pkg.build()?)?;
        for spec in specifiers {
            if spec_to_pid.insert(spec, pid).is_some() {
                return Err(Error::DuplicateSpecifiers(format!("{}@{}", spec.0, spec.1)));
            }
        }
        ekey_to_pid.insert(key, pid);
        pid_to_entry.insert(pid, entry);
        if let Some(mjam::Resolved::Remote(src)) = resolved {
            package_sources.insert(pid, src);
        }
    }

    for ((patched_name, patched_sv), (patch_path, patch_sv)) in &patched_packages {
        let og_pid = spec_to_pid.get(&(patched_name, patched_sv)).unwrap();
        let mut pkg = package_builder_copy
            .remove(&(&patched_name, &patched_sv))
            .unwrap();
        let patch = PackagePatchBuilder::new(patch_path.to_string()).build()?;
        let deriv_meta =
            PackageDerivationMetaBuilder::new(PackageDerivation::Patch(patch), *og_pid);
        pkg.derived(deriv_meta.build()?);
        let patched_pid = chastefile.add_package(pkg.build()?)?;
        spec_to_pid.insert((patched_name, patch_sv), patched_pid);
    }

    let mut dep_children: HashMap<PackageID, Vec<PackageID>> = HashMap::new();

    let mut mpji = member_package_jsons
        .iter()
        .enumerate()
        .map(|(idx, (_, p))| (*mpj_idx_to_pid.get(&idx).unwrap(), p.get()));
    let mut root_done = false;
    loop {
        let Some((pid, package_json)) = mpji.next().or_else(|| {
            if !root_done {
                root_done = true;
                Some((root_pid, &root_package_json))
            } else {
                None
            }
        }) else {
            break;
        };
        for (kind, dependencies) in [
            (DependencyKind::Dependency, &package_json.dependencies),
            (
                DependencyKind::DevDependency,
                &package_json.dev_dependencies,
            ),
            (
                DependencyKind::OptionalDependency,
                &package_json.optional_dependencies,
            ),
            (
                DependencyKind::PeerDependency,
                &package_json.peer_dependencies,
            ),
        ] {
            for (dep_name, dep_svs) in dependencies {
                if let Some(dep) = resolve_dependency(
                    (dep_name, dep_svs),
                    kind,
                    &resolutions,
                    pid,
                    &[],
                    &spec_to_pid,
                    &dep_children,
                    &package_sources,
                    &pid_to_entry,
                )? {
                    if !kind.is_peer() {
                        dep_children
                            .get_mut(&pid)
                            .map(|l| l.push(dep.on))
                            .unwrap_or_else(|| {
                                dep_children.insert(pid, vec![dep.on]);
                            });
                    }
                    chastefile.add_dependency(dep);
                }
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
            let parent_specifiers = LazyLock::new(|| mjam::specifiers(key).unwrap());
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
                    &dep_children,
                    &package_sources,
                    &pid_to_entry,
                )? {
                    if !kind.is_peer() {
                        dep_children
                            .get_mut(&from_pid)
                            .map(|l| l.push(dep.on))
                            .unwrap_or_else(|| {
                                dep_children.insert(from_pid, vec![dep.on]);
                            });
                    }
                    chastefile.add_dependency(dep);
                }
            }
        }
    }

    chastefile.build().map_err(Error::ChasteError)
}

fn resolve_dependency<'y>(
    (dep_name, dep_svs): (&str, &str),
    kind: DependencyKind,
    resolutions: &Resolutions,
    from_pid: PackageID,
    parent_specifiers: &[(&str, &str)],
    spec_to_pid: &BTreeMap<(&str, &str), PackageID>,
    dep_children: &HashMap<PackageID, Vec<PackageID>>,
    package_sources: &HashMap<PackageID, PackageSource>,
    pid_to_entry: &HashMap<PackageID, &'y types::Entry<'y>>,
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
        resolve_peer_dependency(
            (
                alias.as_ref().map(|n| n.as_ref()).unwrap_or(dep_name),
                alias_spec,
            ),
            override_spec,
            override_svs,
            candidates.collect(),
            dep_children,
            from_pid,
            package_sources,
            pid_to_entry,
        )?
    } else {
        let candidates: Vec<_> = candidates
            .filter(|((_, s), _)| is_same_svs_zpm(alias_spec, s))
            .map(|(_, pid)| *pid)
            .unique()
            .collect();
        match *candidates {
            [] if kind.is_optional() => return Ok(None),
            [] => return Err(Error::DependencyNotFound(format!("{dep_name}@{dep_svs}"))),
            [pid] => Some(pid),
            [..] => return Err(Error::AmbiguousResolution(format!("{dep_name}@{dep_svs}"))),
        }
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
    (_dep_name, dep_svs): (&'y str, &'y str),
    override_spec: Option<&'y str>,
    override_svs: &SourceVersionSpecifier,
    candidate_entries: Vec<(&(&'y str, &'y str), &PackageID)>,
    dep_children: &HashMap<PackageID, Vec<PackageID>>,
    from_pid: PackageID,
    package_sources: &HashMap<PackageID, PackageSource>,
    pid_to_entry: &HashMap<PackageID, &'y types::Entry<'y>>,
) -> Result<Option<PackageID>> {
    let candidate_pids: Vec<PackageID> = candidate_entries
        .iter()
        .map(|(_, pid)| **pid)
        .unique()
        .collect();

    // If there's just one candidate to consider, it's easy.
    if let [pid] = *candidate_pids {
        return Ok(Some(pid));
    }
    // Peer dependencies can be optional or unfulfilled.
    if candidate_entries.len() == 0 {
        return Ok(None);
    }
    // If an SVS is overridden through package.json "resolutions" field,
    // it takes that field's value into its descriptors.
    if let Some(evaluated_svs) = override_spec {
        if let [(_, pid)] = *candidate_entries
            .iter()
            .filter(|((_, d_s), _)| is_same_svs_zpm(evaluated_svs, d_s))
            .unique_by(|(_, pid)| **pid)
            .collect::<Vec<_>>()
        {
            return Ok(Some(**pid));
        }
    }

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
        .unique()
        .collect::<Vec<_>>();
    if let [pid] = *candidate_siblings {
        return Ok(Some(*pid));
    }

    // If a package is requested somewhere else with the same specifier.
    let same_spec_candidates = candidate_entries
        .iter()
        .filter(|((_, s), _)| is_same_svs_zpm(dep_svs, s))
        .unique_by(|(_, pid)| **pid)
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
    if let Some(range) = override_svs.npm_range() {
        let mut satisfying_candidates = candidate_entries
            .iter()
            .unique_by(|(_, pid)| **pid)
            .filter_map(|(_, pid)| {
                if package_sources
                    .get(pid)
                    .is_some_and(|s| s.source_type() == PackageSourceType::Npm)
                {
                    let entry = pid_to_entry.get(*pid).unwrap();
                    Some((
                        pid,
                        PackageVersion::parse(entry.resolution.version).unwrap(),
                    ))
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
