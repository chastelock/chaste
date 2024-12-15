// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::path::Path;
use std::{fs, str};

use chaste_types::{
    ssri, Chastefile, ChastefileBuilder, DependencyBuilder, DependencyKind, InstallationBuilder,
    Integrity, Package, PackageBuilder, PackageID, PackageName, PackageSource, PackageVersion,
    SourceVersionDescriptor, PACKAGE_JSON_FILENAME,
};
use nom::branch::alt;
use nom::bytes::complete::{tag, take_while1};
use nom::combinator::{eof, opt, verify};
use nom::sequence::tuple;
use yarn_lock_parser as yarn;

pub use crate::error::{Error, Result};
use crate::types::PackageJson;

mod error;
mod types;

pub static LOCKFILE_NAME: &str = "yarn.lock";

fn is_registry_url<'a>(name: &'a str, version: &'a str, input: &'a str) -> bool {
    tuple((
        tag::<&str, &str, ()>("https://registry.yarnpkg.com/"),
        tag(name),
        tag("/-/"),
        tag(name),
        tag("-"),
        tag(version),
        tag(".tgz"),
        eof,
    ))(input)
    .is_ok()
}

fn is_github_svd(input: &str) -> bool {
    tuple((
        opt(tag::<&str, &str, ()>("github:")),
        take_while1(|c: char| c.is_ascii_alphanumeric() || c == '-'),
        tag("/"),
        verify(
            take_while1(|c: char| c.is_ascii_alphanumeric() || ['-', '.', '_'].contains(&c)),
            |name: &str| !name.starts_with("."),
        ),
        alt((tag("#"), eof)),
    ))(input)
    .is_ok()
}

fn parse_source_url(entry: &yarn::Entry, url: &str) -> Result<Option<PackageSource>> {
    Ok(if is_registry_url(entry.name, entry.version, url) {
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
    } else if entry.descriptors.iter().all(|(_, svd)| {
        svd.starts_with("https://") || svd.starts_with("http://") || is_github_svd(svd)
    }) {
        Some(PackageSource::TarballURL {
            url: url.to_string(),
        })

    // Not an arbitrary tarball? If it's valid semver, it's probably a custom registry.
    } else if entry
        .descriptors
        .iter()
        .all(|(_, svd)| PackageVersion::parse(svd).is_ok())
    {
        Some(PackageSource::Npm)
    } else {
        // TODO: find any cases falling here
        None
    })
}

fn parse_source<'a>(entry: &'a yarn::Entry) -> Result<Option<(PackageSource, Option<&'a str>)>> {
    let (url, hash) = entry
        .resolved
        .rsplit_once("#")
        .map(|(u, h)| (u, Some(h)))
        .unwrap_or((entry.resolved, None));
    let Some(source) = parse_source_url(entry, url)? else {
        return Ok(None);
    };
    Ok(Some(match source {
        PackageSource::Npm | PackageSource::TarballURL { .. } => (source, hash),
        _ => (source, None),
    }))
}

fn parse_package(entry: &yarn::Entry) -> Result<PackageBuilder> {
    let mut pkg = PackageBuilder::new(
        Some(PackageName::new(entry.name.to_string())?),
        Some(entry.version.to_string()),
    );
    let mut integrity: Integrity = entry.integrity.parse()?;
    if let Some((source, maybe_sha1_hex)) = parse_source(entry)? {
        if let Some(sha1_hex) = maybe_sha1_hex {
            integrity = integrity.concat(Integrity::from_hex(sha1_hex, ssri::Sha1)?);
        }
        pkg.source(source);
    }
    pkg.integrity(integrity);
    Ok(pkg)
}

fn find_dep_pid<'a, S>(
    descriptor: &'a (S, S),
    yarn_lock: &'a yarn::Lockfile<'a>,
    index_to_pid: &HashMap<usize, PackageID>,
    member_package_jsons: &'a [(&str, PackageJson)],
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

fn resolve<'a>(yarn_lock: yarn::Lockfile<'a>, root_dir: &Path) -> Result<Chastefile> {
    if yarn_lock.version != 1 {
        return Err(Error::UnknownLockfileVersion(yarn_lock.version));
    }

    let root_package_contents = fs::read_to_string(root_dir.join(PACKAGE_JSON_FILENAME))?;
    let root_package_json: PackageJson = serde_json::from_str(&root_package_contents)?;

    let mut member_package_jsons: Vec<(&str, PackageJson)> = Vec::new();
    let mut mpj_idx_to_pid: HashMap<usize, PackageID> = HashMap::new();

    let mut chastefile_builder = ChastefileBuilder::new();
    let mut index_to_pid: HashMap<usize, PackageID> =
        HashMap::with_capacity(yarn_lock.entries.len());

    // Oh, workspaces are not checked in either.
    if let Some(workspaces) = &root_package_json.workspaces {
        let member_packages = workspaces
            .iter()
            .map(|workspace| -> Result<(&str, PackageJson)> {
                let member_package_json_contents = fs::read_to_string(
                    root_dir
                        .join(workspace.as_ref())
                        .join(PACKAGE_JSON_FILENAME),
                )?;
                Ok((
                    workspace.as_ref(),
                    serde_json::from_str(&member_package_json_contents)?,
                ))
            })
            .collect::<Result<Vec<(&str, PackageJson)>>>()?;
        member_package_jsons.extend(member_packages);
    }
    // The funny part of this is that the root package is not checked in.
    // So we have to parse package.json and add it manually.
    let root_package = pkg_json_to_package(&root_package_json)?;
    let root_pid = chastefile_builder.add_package(root_package)?;
    chastefile_builder.set_root_package_id(root_pid)?;
    let root_install = InstallationBuilder::new(root_pid, "".to_string()).build()?;
    chastefile_builder.add_package_installation(root_install);
    for (idx, (workspace_path, member_package_json)) in member_package_jsons.iter().enumerate() {
        let member_package = pkg_json_to_package(member_package_json)?;
        let member_pid = chastefile_builder.add_package(member_package)?;
        mpj_idx_to_pid.insert(idx, member_pid);
        chastefile_builder.set_as_workspace_member(member_pid)?;
        chastefile_builder.add_package_installation(
            InstallationBuilder::new(member_pid, workspace_path.to_string()).build()?,
        );
    }

    // Now, add everything else.
    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let pkg = parse_package(entry)?;
        let pid = chastefile_builder.add_package(pkg.build()?)?;
        index_to_pid.insert(index, pid);
    }

    // Now mark dependencies of the root and workspace members packages. All by each type.
    for package_json in [&root_package_json]
        .into_iter()
        .chain(member_package_jsons.iter().map(|(_, pj)| pj))
    {
        for dep_descriptor in &package_json.dependencies {
            let dep_pid = find_dep_pid(
                &dep_descriptor,
                &yarn_lock,
                &index_to_pid,
                &member_package_jsons,
                &mpj_idx_to_pid,
            )?;
            let mut dep = DependencyBuilder::new(DependencyKind::Dependency, root_pid, dep_pid);
            dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
            chastefile_builder.add_dependency(dep.build());
        }
        for dep_descriptor in &package_json.dev_dependencies {
            let dep_pid = find_dep_pid(
                &dep_descriptor,
                &yarn_lock,
                &index_to_pid,
                &member_package_jsons,
                &mpj_idx_to_pid,
            )?;
            let mut dep = DependencyBuilder::new(DependencyKind::DevDependency, root_pid, dep_pid);
            dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
            chastefile_builder.add_dependency(dep.build());
        }
        for dep_descriptor in &package_json.peer_dependencies {
            let dep_pid = find_dep_pid(
                &dep_descriptor,
                &yarn_lock,
                &index_to_pid,
                &member_package_jsons,
                &mpj_idx_to_pid,
            )?;
            let mut dep = DependencyBuilder::new(DependencyKind::PeerDependency, root_pid, dep_pid);
            dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
            chastefile_builder.add_dependency(dep.build());
        }
        for dep_descriptor in &package_json.optional_dependencies {
            let dep_pid = find_dep_pid(
                &dep_descriptor,
                &yarn_lock,
                &index_to_pid,
                &member_package_jsons,
                &mpj_idx_to_pid,
            )?;
            let mut dep =
                DependencyBuilder::new(DependencyKind::OptionalDependency, root_pid, dep_pid);
            dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
            chastefile_builder.add_dependency(dep.build());
        }
    }

    // Finally, dependencies of dependencies.
    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let from_pid = index_to_pid.get(&index).unwrap();
        for dep_descriptor in &entry.dependencies {
            let dep_pid = find_dep_pid(
                &dep_descriptor,
                &yarn_lock,
                &index_to_pid,
                &member_package_jsons,
                &mpj_idx_to_pid,
            )?;
            // devDependencies of non-root packages are not written to the lockfile.
            // It might be peer and/or optional. But in that case, it got added here
            // by root and/or another dependency.
            let mut dep = DependencyBuilder::new(DependencyKind::Dependency, *from_pid, dep_pid);
            dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
            chastefile_builder.add_dependency(dep.build());
        }
    }
    Ok(chastefile_builder.build()?)
}

pub fn parse<P>(root_dir: P) -> Result<Chastefile>
where
    P: AsRef<Path>,
{
    let root_dir = root_dir.as_ref();
    let lockfile_contents = fs::read_to_string(root_dir.join(LOCKFILE_NAME))?;
    let yarn_lock: yarn::Lockfile = yarn::parse_str(&lockfile_contents)?;
    resolve(yarn_lock, root_dir)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::LazyLock;

    use chaste_types::{Chastefile, Package, PackageID, PackageSourceType};

    use super::{is_github_svd, parse, Result};

    #[test]
    fn github_cvd() -> Result<()> {
        assert!(is_github_svd("isaacs/minimatch#v10.0.1"));
        assert!(is_github_svd("github:isaacs/minimatch#v10.0.1"));
        assert!(is_github_svd("isaacs/minimatch"));

        Ok(())
    }

    static TEST_WORKSPACES: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("test_workspaces"));

    fn test_workspace(name: &str) -> Result<Chastefile> {
        parse(TEST_WORKSPACES.join(name))
    }

    #[test]
    fn v1_basic() -> Result<()> {
        let chastefile = test_workspace("v1_basic")?;
        let rec_deps = chastefile.recursive_package_dependencies(chastefile.root_package_id());
        assert_eq!(rec_deps.len(), 5);
        assert!(rec_deps
            .iter()
            .map(|d| chastefile.package(d.on))
            .all(|p| p.source_type() == Some(PackageSourceType::Npm)));
        Ok(())
    }

    #[test]
    fn v1_git_ssh() -> Result<()> {
        let chastefile = test_workspace("v1_git_ssh")?;
        assert_eq!(
            chastefile
                .recursive_package_dependencies(chastefile.root_package_id())
                .len(),
            1
        );
        let root_package_dependencies = chastefile.root_package_dependencies();
        let semver_dep = root_package_dependencies.first().unwrap();
        let svd = semver_dep.svd().unwrap();
        assert!(svd.is_git());
        assert_eq!(svd.ssh_path_sep(), Some("/"));
        let semver = chastefile.package(semver_dep.on);
        assert_eq!(semver.name().unwrap(), "node-semver");
        assert_eq!(semver.version().unwrap().to_string(), "7.6.3");
        assert_eq!(semver.source_type(), Some(PackageSourceType::Git));
        assert_eq!(semver.integrity().hashes.len(), 0);
        Ok(())
    }

    #[test]
    fn v1_git_url() -> Result<()> {
        let chastefile = test_workspace("v1_git_url")?;
        assert_eq!(
            chastefile
                .recursive_package_dependencies(chastefile.root_package_id())
                .len(),
            3
        );
        let root_package_dependencies = chastefile.root_package_dependencies();
        let minimatch_dep = root_package_dependencies.first().unwrap();
        let minimatch = chastefile.package(minimatch_dep.on);
        assert_eq!(minimatch.name().unwrap(), "minimatch");
        assert_eq!(minimatch.version().unwrap().to_string(), "10.0.1");
        assert_eq!(minimatch.source_type(), Some(PackageSourceType::Git));
        assert_eq!(minimatch.integrity().hashes.len(), 0);
        Ok(())
    }

    #[test]
    fn v1_github_ref() -> Result<()> {
        let chastefile = test_workspace("v1_github_ref")?;
        assert_eq!(
            chastefile
                .recursive_package_dependencies(chastefile.root_package_id())
                .len(),
            4
        );
        let root_package_dependencies = chastefile.root_package_dependencies();
        let mut root_dep_packages: Vec<&Package> = root_package_dependencies
            .iter()
            .map(|d| chastefile.package(d.on))
            .collect();
        assert_eq!(root_dep_packages.len(), 2);
        root_dep_packages.sort_unstable_by_key(|p| p.name());

        let package = root_dep_packages[0];
        assert_eq!(package.name().unwrap(), "minimatch");
        assert_eq!(package.version().unwrap().to_string(), "10.0.1");
        assert_eq!(package.source_type(), Some(PackageSourceType::TarballURL));
        assert_eq!(package.integrity().hashes.len(), 0);

        let package = root_dep_packages[1];
        assert_eq!(package.name().unwrap(), "node-semver");
        assert_eq!(package.version().unwrap().to_string(), "7.6.3");
        assert_eq!(package.source_type(), Some(PackageSourceType::TarballURL));
        assert_eq!(package.integrity().hashes.len(), 0);

        Ok(())
    }

    #[test]
    fn v1_scope_registry() -> Result<()> {
        let chastefile = test_workspace("v1_scope_registry")?;
        let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
        let empty_pkg = chastefile.package(empty_pid);
        assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
        assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
        assert_eq!(empty_pkg.integrity().hashes.len(), 2);
        assert_eq!(empty_pkg.source_type(), Some(PackageSourceType::Npm));

        Ok(())
    }

    #[test]
    fn v1_workspace_basic() -> Result<()> {
        let chastefile = test_workspace("v1_workspace_basic")?;
        dbg!(&chastefile);
        assert_eq!(chastefile.packages().len(), 4);
        let [(balls_pid, _balls_pkg)] = *chastefile
            .packages_with_ids()
            .into_iter()
            .filter(|(_, p)| p.name().is_some_and(|n| n == "@chastelock/balls"))
            .collect::<Vec<(PackageID, &Package)>>()
        else {
            panic!();
        };
        let [(ligma_pid, _ligma_pkg)] = *chastefile
            .packages_with_ids()
            .into_iter()
            .filter(|(_, p)| p.name().is_some_and(|n| n == "ligma-api"))
            .collect::<Vec<(PackageID, &Package)>>()
        else {
            panic!();
        };
        let workspace_member_ids = chastefile.workspace_member_ids();
        assert_eq!(workspace_member_ids.len(), 2);
        assert!(
            workspace_member_ids.contains(&balls_pid) && workspace_member_ids.contains(&ligma_pid)
        );
        let balls_installations = chastefile.package_installations(balls_pid);
        // There are 2: where the package is, and a link in "node_modules/{pkg.name}", but the latter is not tracked here yet.
        assert_eq!(balls_installations.len(), 1);
        let balls_install_paths = balls_installations
            .iter()
            .map(|i| i.path())
            .collect::<Vec<&str>>();
        assert_eq!(balls_install_paths, ["balls"]);

        Ok(())
    }
}
