// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::str;

use chaste_types::{
    ssri, Chastefile, ChastefileBuilder, DependencyBuilder, DependencyKind, InstallationBuilder,
    Integrity, PackageBuilder, PackageID, PackageName, PackageSource, PackageVersion,
    SourceVersionDescriptor,
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

pub static LOCKFILE_NAME: &'static str = "yarn.lock";

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

fn is_github_svd<'a>(input: &'a str) -> bool {
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
    if let Some((source, maybe_sha1_hex)) = parse_source(&entry)? {
        if let Some(sha1_hex) = maybe_sha1_hex {
            integrity = integrity.concat(Integrity::from_hex(sha1_hex, ssri::Sha1)?);
        }
        pkg.source(source);
    }
    pkg.integrity(integrity);
    Ok(pkg)
}

fn find_dep_index<'a, S>(yarn_lock: &'a yarn::Lockfile<'a>, descriptor: &'a (S, S)) -> Result<usize>
where
    S: AsRef<str>,
{
    let real_descriptor = (descriptor.0.as_ref(), descriptor.1.as_ref());
    let Some((dep_index, _)) = yarn_lock
        .entries
        .iter()
        .enumerate()
        .find(|(_, e)| e.descriptors.contains(&real_descriptor))
    else {
        return Err(Error::DependencyNotFound(format!(
            "{0}@{1}",
            real_descriptor.0, real_descriptor.1
        )));
    };
    Ok(dep_index)
}

fn resolve<'a>(
    package_json: &'a PackageJson<'a>,
    yarn_lock: yarn::Lockfile<'a>,
) -> Result<Chastefile> {
    if package_json.workspaces.is_some() {
        return Err(Error::RootHasWorkspaces());
    }
    if yarn_lock.version != 1 {
        return Err(Error::UnknownLockfileVersion(yarn_lock.version));
    }

    let mut chastefile_builder = ChastefileBuilder::new();
    let mut index_to_pid: HashMap<usize, PackageID> =
        HashMap::with_capacity(yarn_lock.entries.len());

    // The funny part of this is that the root package is not checked in.
    // So we have to parse package.json and add it manually.
    let root_package = PackageBuilder::new(
        package_json
            .name
            .as_ref()
            .map(|s| PackageName::new(s.to_string()))
            .transpose()?,
        package_json.version.as_ref().map(|s| s.to_string()),
    );
    let root_pid = chastefile_builder.add_package(root_package.build()?);
    chastefile_builder.set_root_package_id(root_pid)?;
    let root_install = InstallationBuilder::new(root_pid, "".to_string()).build()?;
    chastefile_builder.add_package_installation(root_install);

    // Now, add everything else.
    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let pkg = parse_package(entry)?;
        let pid = chastefile_builder.add_package(pkg.build()?);
        index_to_pid.insert(index, pid);
    }

    // Now mark dependencies of root package. All by each type.
    for dep_descriptor in &package_json.dependencies {
        let dep_index = find_dep_index(&yarn_lock, &dep_descriptor)?;
        let dep_pid = index_to_pid.get(&dep_index).unwrap();
        let mut dep = DependencyBuilder::new(DependencyKind::Dependency, root_pid, *dep_pid);
        dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
        chastefile_builder.add_dependency(dep.build());
    }
    for dep_descriptor in &package_json.dev_dependencies {
        let dep_index = find_dep_index(&yarn_lock, &dep_descriptor)?;
        let dep_pid = index_to_pid.get(&dep_index).unwrap();
        let mut dep = DependencyBuilder::new(DependencyKind::DevDependency, root_pid, *dep_pid);
        dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
        chastefile_builder.add_dependency(dep.build());
    }
    for dep_descriptor in &package_json.peer_dependencies {
        let dep_index = find_dep_index(&yarn_lock, &dep_descriptor)?;
        let dep_pid = index_to_pid.get(&dep_index).unwrap();
        let mut dep = DependencyBuilder::new(DependencyKind::PeerDependency, root_pid, *dep_pid);
        dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
        chastefile_builder.add_dependency(dep.build());
    }
    for dep_descriptor in &package_json.optional_dependencies {
        let dep_index = find_dep_index(&yarn_lock, &dep_descriptor)?;
        let dep_pid = index_to_pid.get(&dep_index).unwrap();
        let mut dep =
            DependencyBuilder::new(DependencyKind::OptionalDependency, root_pid, *dep_pid);
        dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
        chastefile_builder.add_dependency(dep.build());
    }

    // Finally, dependencies of dependencies.
    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let from_pid = index_to_pid.get(&index).unwrap();
        for dep_descriptor in &entry.dependencies {
            let dep_index = find_dep_index(&yarn_lock, dep_descriptor)?;
            let dep_pid = index_to_pid.get(&dep_index).unwrap();
            // devDependencies of non-root packages are not written to the lockfile.
            // It might be peer and/or optional. But in that case, it got added here
            // by root and/or another dependency.
            let mut dep = DependencyBuilder::new(DependencyKind::Dependency, *from_pid, *dep_pid);
            dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
            chastefile_builder.add_dependency(dep.build());
        }
    }
    Ok(chastefile_builder.build()?)
}

pub fn from_str(package_json_contents: &str, yarn_lock_contents: &str) -> Result<Chastefile> {
    let package_json: PackageJson = serde_json::from_str(package_json_contents)?;
    let yarn_lock = yarn::parse_str(yarn_lock_contents)?;
    resolve(&package_json, yarn_lock)
}

pub fn from_slice(pv: &[u8], yv: &[u8]) -> Result<Chastefile> {
    from_str(str::from_utf8(pv)?, str::from_utf8(yv)?)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chaste_types::{Chastefile, Package, PackageSourceType};

    use super::{from_str, is_github_svd, Result};

    #[test]
    fn github_cvd() -> Result<()> {
        assert!(is_github_svd("isaacs/minimatch#v10.0.1"));
        assert!(is_github_svd("github:isaacs/minimatch#v10.0.1"));
        assert!(is_github_svd("isaacs/minimatch"));

        Ok(())
    }

    fn test_workspace(name: &str) -> Result<Chastefile> {
        let package_json = fs::read_to_string(format!("test_workspaces/{name}/package.json"))?;
        let yarn_lock = fs::read_to_string(format!("test_workspaces/{name}/yarn.lock"))?;
        from_str(&package_json, &yarn_lock)
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
}
