// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::str;

use chaste_types::{
    Chastefile, ChastefileBuilder, DependencyBuilder, DependencyKind, InstallationBuilder,
    PackageBuilder, PackageID, PackageName,
};
use types::PackageJson;
use yarn_lock_parser as yarn;

pub use crate::error::{Error, Result};

mod error;
mod types;

pub static LOCKFILE_NAME: &'static str = "yarn.lock";

fn parse_package(entry: &yarn::Entry) -> Result<PackageBuilder> {
    let mut pkg = PackageBuilder::new(
        Some(PackageName::new(entry.name.to_string())?),
        Some(entry.version.to_string()),
    );
    pkg.integrity(entry.integrity.parse()?);
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
    dbg!(&yarn_lock);

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
        chastefile_builder.add_dependency(
            DependencyBuilder::new(DependencyKind::Dependency, root_pid, *dep_pid).build(),
        );
    }
    for dep_descriptor in &package_json.dev_dependencies {
        let dep_index = find_dep_index(&yarn_lock, &dep_descriptor)?;
        let dep_pid = index_to_pid.get(&dep_index).unwrap();
        chastefile_builder.add_dependency(
            DependencyBuilder::new(DependencyKind::DevDependency, root_pid, *dep_pid).build(),
        );
    }
    for dep_descriptor in &package_json.peer_dependencies {
        let dep_index = find_dep_index(&yarn_lock, &dep_descriptor)?;
        let dep_pid = index_to_pid.get(&dep_index).unwrap();
        chastefile_builder.add_dependency(
            DependencyBuilder::new(DependencyKind::PeerDependency, root_pid, *dep_pid).build(),
        );
    }
    for dep_descriptor in &package_json.optional_dependencies {
        let dep_index = find_dep_index(&yarn_lock, &dep_descriptor)?;
        let dep_pid = index_to_pid.get(&dep_index).unwrap();
        chastefile_builder.add_dependency(
            DependencyBuilder::new(DependencyKind::OptionalDependency, root_pid, *dep_pid).build(),
        );
    }

    // Finally, dependencies of dependencies.
    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let from_pid = index_to_pid.get(&index).unwrap();
        for dep_descriptor in &entry.dependencies {
            let dep_index = find_dep_index(&yarn_lock, dep_descriptor)?;
            let dep_pid = index_to_pid.get(&dep_index).unwrap();
            chastefile_builder.add_dependency(
                DependencyBuilder::new(
                    // devDependencies of non-root packages are not written to the lockfile.
                    // It might be peer and/or optional. But in that case, it got added here
                    // by root and/or another dependency.
                    DependencyKind::Dependency,
                    *from_pid,
                    *dep_pid,
                )
                .build(),
            );
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

    use chaste_types::{Chastefile, PackageSourceType};

    use super::{from_str, Result};

    fn test_workspace(name: &str) -> Result<Chastefile> {
        let package_json = fs::read_to_string(format!("test_workspaces/{name}/package.json"))?;
        let yarn_lock = fs::read_to_string(format!("test_workspaces/{name}/yarn.lock"))?;
        from_str(&package_json, &yarn_lock)
    }

    #[test]
    fn v1_basic() -> Result<()> {
        let chastefile = test_workspace("v1_basic")?;
        assert_eq!(
            chastefile
                .recursive_package_dependencies(chastefile.root_package_id())
                .len(),
            5
        );
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
        // TODO: mark this correctly
        assert_eq!(minimatch.source_type(), None);
        Ok(())
    }
}
