// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::{fs, str};

use chaste_types::{
    ssri, Chastefile, ChastefileBuilder, Checksums, DependencyBuilder, DependencyKind,
    InstallationBuilder, Integrity, ModulePath, Package, PackageBuilder, PackageID, PackageName,
    PackageNameBorrowed, PackageSource, PackageVersion, QuirksMode, SourceVersionSpecifier,
    PACKAGE_JSON_FILENAME, ROOT_MODULE_PATH,
};
use globreeks::Globreeks;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_while1};
use nom::combinator::{eof, opt, verify};
use nom::Parser;
use walkdir::WalkDir;
use yarn_lock_parser as yarn;

use crate::classic::types::PackageJson;
use crate::error::{Error, Result};

mod types;

static QUIRKS: QuirksMode = QuirksMode::Yarn(1);

fn is_registry_url<'a>(name: PackageNameBorrowed<'a>, version: &'a str, input: &'a str) -> bool {
    (
        alt((tag("https://"), tag("http://"))),
        alt((
            tag("registry.yarnpkg.com"),
            tag("registry.npmjs.org"),
            tag("registry.npmjs.com"),
        )),
        tag::<&str, &str, ()>("/"),
        tag(name.as_ref()),
        tag("/-/"),
        tag(name.name_rest()),
        tag("-"),
        tag(version),
        tag(".tgz"),
        eof,
    )
        .parse(input)
        .is_ok()
}

fn is_github_svs(input: &str) -> bool {
    (
        opt(tag::<&str, &str, ()>("github:")),
        take_while1(|c: char| c.is_ascii_alphanumeric() || c == '-'),
        tag("/"),
        verify(
            take_while1(|c: char| c.is_ascii_alphanumeric() || ['-', '.', '_'].contains(&c)),
            |name: &str| !name.starts_with("."),
        ),
        alt((tag("#"), eof)),
    )
        .parse(input)
        .is_ok()
}

fn parse_source_url(
    entry: &yarn::Entry,
    package_name: PackageNameBorrowed<'_>,
    url: &str,
) -> Result<Option<PackageSource>> {
    Ok(if is_registry_url(package_name, entry.version, url) {
        Some(PackageSource::Npm)
    } else if url.ends_with(".git") {
        Some(PackageSource::Git {
            url: url.to_string(),
        })

    // Check descriptors whether they are:
    // a) a tarball URL,
    // b) the special GitHub tag (in yarn, it resolves to tarballs).
    //
    // XXX: This might be wrong with overrides.
    } else if entry.descriptors.iter().all(|(_, svs)| {
        svs.starts_with("https://") || svs.starts_with("http://") || is_github_svs(svs)
    }) {
        Some(PackageSource::TarballURL {
            url: url.to_string(),
        })

    // Not an arbitrary tarball? If it's valid semver, it's probably a custom registry.
    } else if entry
        .descriptors
        .iter()
        .all(|(_, svs)| PackageVersion::parse(svs).is_ok())
    {
        Some(PackageSource::Npm)
    } else {
        // TODO: find any cases falling here
        None
    })
}

fn parse_source<'a>(
    entry: &'a yarn::Entry,
    package_name: PackageNameBorrowed<'a>,
) -> Result<Option<(PackageSource, Option<&'a str>)>> {
    let (url, hash) = entry
        .resolved
        .rsplit_once("#")
        .map(|(u, h)| (u, Some(h)))
        .unwrap_or((entry.resolved, None));
    let Some(source) = parse_source_url(entry, package_name, url)? else {
        return Ok(None);
    };
    Ok(Some(match source {
        PackageSource::Npm | PackageSource::TarballURL { .. } => (source, hash),
        _ => (source, None),
    }))
}

fn parse_package(entry: &yarn::Entry) -> Result<PackageBuilder> {
    let first_desc = entry.descriptors.first().unwrap();
    let name = if first_desc.1.starts_with("npm:") {
        let svs = SourceVersionSpecifier::with_quirks(first_desc.1.to_string(), QUIRKS)?;
        if let Some(aliased_name) = svs.aliased_package_name() {
            aliased_name.to_owned()
        } else {
            PackageName::new(first_desc.0.to_string())?
        }
    } else {
        PackageName::new(first_desc.0.to_string())?
    };
    let mut integrity: Integrity = entry.integrity.parse()?;
    let source = if let Some((source, maybe_sha1_hex)) = parse_source(entry, name.as_borrowed())? {
        if let Some(sha1_hex) = maybe_sha1_hex {
            integrity = integrity.concat(Integrity::from_hex(sha1_hex, ssri::Sha1)?);
        }
        Some(source)
    } else {
        None
    };
    let mut pkg = PackageBuilder::new(Some(name), Some(entry.version.to_string()));
    if let Some(source) = source {
        pkg.source(source);
    }
    if !integrity.hashes.is_empty() {
        pkg.checksums(Checksums::Tarball(integrity));
    }
    Ok(pkg)
}

fn find_dep_pid<'a, S>(
    descriptor: &'a (S, S),
    yarn_lock: &'a yarn::Lockfile<'a>,
    index_to_pid: &HashMap<usize, PackageID>,
    member_package_jsons: &'a [(Cow<'a, str>, PackageJson)],
    mpj_idx_to_pid: &HashMap<usize, PackageID>,
) -> Result<PackageID>
where
    S: AsRef<str>,
{
    let descriptor = (descriptor.0.as_ref(), descriptor.1.as_ref());
    if let Some((idx, (_, _))) = member_package_jsons
        .iter()
        .enumerate()
        .find(|(_, (_, pj))| pj.name.as_ref().is_some_and(|n| n == descriptor.0))
    {
        Ok(*mpj_idx_to_pid.get(&idx).unwrap())
    } else if let Some((idx, _)) = yarn_lock
        .entries
        .iter()
        .enumerate()
        .find(|(_, e)| e.descriptors.contains(&descriptor))
    {
        Ok(*index_to_pid.get(&idx).unwrap())
    } else {
        Err(Error::DependencyNotFound(format!(
            "{0}@{1}",
            descriptor.0, descriptor.1
        )))
    }
}

fn pkg_json_to_package<'a>(package_json: &'a PackageJson<'a>) -> Result<Package> {
    PackageBuilder::new(
        package_json
            .name
            .as_ref()
            .map(|s| PackageName::new(s.to_string()))
            .transpose()?,
        package_json.version.as_ref().map(|s| s.to_string()),
    )
    .build()
    .map_err(Error::ChasteError)
}

pub(crate) fn resolve(yarn_lock: yarn::Lockfile<'_>, root_dir: &Path) -> Result<Chastefile> {
    let root_package_contents = fs::read_to_string(root_dir.join(PACKAGE_JSON_FILENAME))?;
    let root_package_json: PackageJson = serde_json::from_str(&root_package_contents)?;

    let mut member_package_jsons: Vec<(Cow<'_, str>, PackageJson)> = Vec::new();
    let mut mpj_idx_to_pid: HashMap<usize, PackageID> = HashMap::new();

    let mut chastefile_builder = ChastefileBuilder::new();
    let mut index_to_pid: HashMap<usize, PackageID> =
        HashMap::with_capacity(yarn_lock.entries.len());

    // Oh, workspaces are not checked in either.
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

            let member_package_json_contents = fs::read_to_string(&absolute_path)
                .map_err(|e| Error::IoInWorkspace(e, absolute_path.to_path_buf()))?;
            member_package_jsons.push((
                // must be owned because its lifetime goes out of scope
                Cow::Owned(relative_workspace_path.to_owned()),
                serde_json::from_str(&member_package_json_contents)?,
            ));
        }
    }
    // The funny part of this is that the root package is not checked in.
    // So we have to parse package.json and add it manually.
    let root_package = pkg_json_to_package(&root_package_json)?;
    let root_pid = chastefile_builder.add_package(root_package)?;
    chastefile_builder.set_root_package_id(root_pid)?;
    let root_install = InstallationBuilder::new(root_pid, ROOT_MODULE_PATH.clone()).build()?;
    chastefile_builder.add_package_installation(root_install);
    for (idx, (workspace_path, member_package_json)) in member_package_jsons.iter().enumerate() {
        let member_package = pkg_json_to_package(member_package_json)?;
        let member_pid = chastefile_builder.add_package(member_package)?;
        mpj_idx_to_pid.insert(idx, member_pid);
        chastefile_builder.set_as_workspace_member(member_pid)?;
        chastefile_builder.add_package_installation(
            InstallationBuilder::new(member_pid, ModulePath::new(workspace_path.to_string())?)
                .build()?,
        );
    }

    // Now, add everything else.
    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let pkg = parse_package(entry)?;
        // When a package is depended on both as a regular npm dependency and via an npm alias,
        // the lockfile duplicates that package. This is specific to v1. Ignore failures and reuse PackageID.
        let pid = match chastefile_builder.add_package(pkg.build()?) {
            Ok(pid) => pid,
            Err(chaste_types::Error::DuplicatePackage(pid)) => pid,
            Err(e) => return Err(Error::ChasteError(e)),
        };
        index_to_pid.insert(index, pid);
    }

    // Now mark dependencies of the root and workspace members packages. All by each type.
    for (member_pid, package_json) in [(root_pid, &root_package_json)].into_iter().chain(
        member_package_jsons
            .iter()
            .enumerate()
            .map(|(midx, (_, pj))| (*mpj_idx_to_pid.get(&midx).unwrap(), pj)),
    ) {
        for (deps, kind_) in [
            (&package_json.dependencies, DependencyKind::Dependency),
            (
                &package_json.dev_dependencies,
                DependencyKind::DevDependency,
            ),
            (
                &package_json.peer_dependencies,
                DependencyKind::PeerDependency,
            ),
            (
                &package_json.optional_dependencies,
                DependencyKind::OptionalDependency,
            ),
        ] {
            for dep_descriptor in deps {
                let kind = match kind_ {
                    DependencyKind::PeerDependency
                        if package_json
                            .peer_dependencies_meta
                            .get(dep_descriptor.0)
                            .is_some_and(|m| m.optional == Some(true)) =>
                    {
                        DependencyKind::OptionalPeerDependency
                    }
                    k => k,
                };
                let dep_pid = find_dep_pid(
                    &dep_descriptor,
                    &yarn_lock,
                    &index_to_pid,
                    &member_package_jsons,
                    &mpj_idx_to_pid,
                )?;
                let mut dep = DependencyBuilder::new(kind, member_pid, dep_pid);
                let svs =
                    SourceVersionSpecifier::with_quirks(dep_descriptor.1.to_string(), QUIRKS)?;
                if svs.aliased_package_name().is_some() {
                    dep.alias_name(PackageName::new(dep_descriptor.0.to_string())?);
                }
                dep.svs(svs);
                chastefile_builder.add_dependency(dep.build());
            }
        }
    }

    // Finally, dependencies of dependencies.
    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let from_pid = index_to_pid.get(&index).unwrap();
        // These should be in berry only.
        debug_assert!(
            entry.peer_dependencies.is_empty() && entry.peer_dependencies_meta.is_empty()
        );
        for (dep_list, dep_kind) in [
            (&entry.dependencies, DependencyKind::Dependency),
            (
                &entry.optional_dependencies,
                DependencyKind::OptionalDependency,
            ),
        ] {
            for dep_descriptor in dep_list {
                let dep_pid = find_dep_pid(
                    dep_descriptor,
                    &yarn_lock,
                    &index_to_pid,
                    &member_package_jsons,
                    &mpj_idx_to_pid,
                )?;
                // devDependencies of non-root packages are not written to the lockfile.
                // It might be peer and/or optional. But in that case, it got added here
                // by root and/or another dependency.
                let mut dep = DependencyBuilder::new(dep_kind, *from_pid, dep_pid);
                let svs =
                    SourceVersionSpecifier::with_quirks(dep_descriptor.1.to_string(), QUIRKS)?;
                if svs.aliased_package_name().is_some() {
                    dep.alias_name(PackageName::new(dep_descriptor.0.to_string())?);
                }
                dep.svs(svs);
                chastefile_builder.add_dependency(dep.build());
            }
        }
    }
    Ok(chastefile_builder.build()?)
}

#[cfg(test)]
mod tests {
    use chaste_types::PackageName;

    use super::{is_github_svs, is_registry_url, Result};

    #[test]
    fn github_cvd() -> Result<()> {
        assert!(is_github_svs("isaacs/minimatch#v10.0.1"));
        assert!(is_github_svs("github:isaacs/minimatch#v10.0.1"));
        assert!(is_github_svs("isaacs/minimatch"));

        Ok(())
    }

    #[test]
    fn registry_url() -> Result<()> {
        assert!(is_registry_url(
            PackageName::new("is-buffer".to_string())?.as_borrowed(),
            "1.1.6",
            "https://registry.yarnpkg.com/is-buffer/-/is-buffer-1.1.6.tgz"
        ));
        assert!(is_registry_url(
            PackageName::new("is-buffer".to_string())?.as_borrowed(),
            "1.1.6",
            "https://registry.npmjs.org/is-buffer/-/is-buffer-1.1.6.tgz"
        ));
        assert!(is_registry_url(
            PackageName::new("@chastelock/recursion-a".to_string())?.as_borrowed(),
            "0.1.0",
            "https://registry.yarnpkg.com/@chastelock/recursion-a/-/recursion-a-0.1.0.tgz"
        ));
        Ok(())
    }
}
