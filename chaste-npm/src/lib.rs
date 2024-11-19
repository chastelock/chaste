// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;

use chaste_types::{
    Chastefile, ChastefileBuilder, Dependency, DependencyKind, PackageBuilder, PackageID,
};

pub use crate::error::{Error, Result};
pub use crate::parsers::PathLexingError;

use crate::parsers::package_name_from_path;
use crate::types::{DependencyTreePackage, PackageLock, PeerDependencyMeta};

mod error;
mod parsers;
mod types;

struct PackageParser<'a> {
    package_lock: &'a PackageLock<'a>,
    chastefile_builder: ChastefileBuilder,
    path_pid: HashMap<&'a Cow<'a, str>, PackageID>,
}

fn parse_package<'a>(
    path: &str,
    tree_package: &'a DependencyTreePackage,
) -> Result<PackageBuilder> {
    let mut name = tree_package.name.as_ref().map(|s| s.to_string());
    // Most packages don't have it as it's implied by the path.
    // So now we have to unimply it.
    if name.is_none() {
        name = package_name_from_path(path)?.map(|s| s.to_string());
    }
    let mut pkg = PackageBuilder::new(name, tree_package.version.as_ref().map(|s| s.to_string()));
    pkg.integrity(tree_package.integrity.as_ref().map(|s| s.to_string()));
    pkg.expected_path(Some(path.to_string()));
    Ok(pkg)
}

fn find_pid<'a>(
    path: &str,
    name: &str,
    path_pid: &HashMap<&'a Cow<'a, str>, PackageID>,
) -> Result<PackageID> {
    let potential_path = match path {
        "" => format!("node_modules/{name}"),
        p => format!("{p}/node_modules/{name}"),
    };
    if let Some(pid) = path_pid.get(&Cow::Borrowed(potential_path.as_str())) {
        return Ok(pid.clone());
    }
    if let Some((parent_path, _)) = path.rsplit_once('/') {
        return find_pid(parent_path, name, path_pid);
    }
    if path != "" {
        return find_pid("", name, path_pid);
    }
    Err(Error::DependencyNotFound(name.to_string()))
}

fn parse_dependencies<'a>(
    path: &str,
    tree_package: &'a DependencyTreePackage,
    path_pid: &HashMap<&'a Cow<'a, str>, PackageID>,
    self_pid: PackageID,
) -> Result<Vec<Dependency>> {
    let mut dependencies = Vec::new();
    for n in tree_package.dependencies.keys() {
        dependencies.push(Dependency {
            kind: DependencyKind::Dependency,
            from: self_pid,
            on: find_pid(path, n, path_pid)?,
        });
    }
    for n in tree_package.dev_dependencies.keys() {
        dependencies.push(Dependency {
            kind: DependencyKind::DevDependency,
            from: self_pid,
            on: find_pid(path, n, path_pid)?,
        });
    }
    for n in tree_package.peer_dependencies.keys() {
        let is_optional = match tree_package.peer_dependencies_meta.get(n) {
            Some(PeerDependencyMeta {
                optional: Some(true),
            }) => true,
            _ => false,
        };
        match find_pid(path, n, path_pid) {
            Ok(pid) => dependencies.push(Dependency {
                kind: if is_optional {
                    DependencyKind::OptionalPeerDependency
                } else {
                    DependencyKind::PeerDependency
                },
                from: self_pid,
                on: pid,
            }),
            // It's optional, ignore.
            Err(Error::DependencyNotFound(_)) if is_optional => {}

            Err(e) => return Err(e),
        }
    }
    for n in tree_package.optional_dependencies.keys() {
        match find_pid(path, n, path_pid) {
            Ok(pid) => dependencies.push(Dependency {
                kind: DependencyKind::OptionalDependency,
                from: self_pid,
                on: pid,
            }),
            // It's optional, ignore.
            Err(Error::DependencyNotFound(_)) => {}

            Err(e) => return Err(e),
        }
    }

    Ok(dependencies)
}

impl<'a> PackageParser<'a> {
    fn new(package_lock: &'a PackageLock) -> Self {
        Self {
            package_lock,
            chastefile_builder: ChastefileBuilder::new(),
            path_pid: HashMap::with_capacity(package_lock.packages.len()),
        }
    }

    fn resolve(mut self) -> Result<Chastefile> {
        for (package_path, tree_package) in self.package_lock.packages.iter() {
            let mut package = parse_package(package_path, tree_package)?;
            if package_path == "" && package.get_name().is_none() {
                package.name(Some(self.package_lock.name.to_string()));
            }
            let pid = self.chastefile_builder.add_package(package.build());
            self.path_pid.insert(package_path, pid);
        }
        for (package_path, tree_package) in self.package_lock.packages.iter() {
            let pid = self.path_pid.get(package_path).unwrap().clone();
            let dependencies = parse_dependencies(package_path, tree_package, &self.path_pid, pid)?;
            self.chastefile_builder
                .add_dependencies(dependencies.into_iter());
        }
        Ok(self.chastefile_builder.build())
    }
}

fn parse_lock(package_lock: &PackageLock) -> Result<Chastefile> {
    if package_lock.lockfile_version != 3 {
        return Err(Error::UnknownLockVersion(package_lock.lockfile_version));
    }
    let parser = PackageParser::new(package_lock);
    let chastefile = parser.resolve()?;
    Ok(chastefile)
}

pub fn from_reader<R>(read: R) -> Result<Chastefile>
where
    R: io::Read,
{
    let package_lock: PackageLock = serde_json::from_reader(read)?;
    parse_lock(&package_lock)
}

pub fn from_slice(v: &[u8]) -> Result<Chastefile> {
    let package_lock: PackageLock = serde_json::from_slice(v)?;
    parse_lock(&package_lock)
}

pub fn from_str(v: &str) -> Result<Chastefile> {
    let package_lock: PackageLock = serde_json::from_str(v)?;
    parse_lock(&package_lock)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{from_reader, Result};

    #[test]
    fn test() -> Result<()> {
        let chastefile = from_reader(fs::File::open("./package-lock.json")?)?;
        dbg!(&chastefile);
        Ok(())
    }
}
