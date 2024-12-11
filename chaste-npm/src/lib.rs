// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;

use chaste_types::{
    Chastefile, ChastefileBuilder, Dependency, DependencyBuilder, DependencyKind,
    InstallationBuilder, PackageBuilder, PackageID, PackageName, PackageSource,
};

pub use crate::error::{Error, Result};
pub use crate::parsers::PathLexingError;

use crate::parsers::package_name_from_path;
use crate::types::{DependencyTreePackage, PackageLock, PeerDependencyMeta};

mod error;
mod parsers;
mod types;

pub static LOCKFILE_NAME: &'static str = "package-lock.json";

struct PackageParser<'a> {
    package_lock: &'a PackageLock<'a>,
    chastefile_builder: ChastefileBuilder,
    path_pid: HashMap<&'a Cow<'a, str>, PackageID>,
}

fn recognize_source<'a>(resolved: &'a Cow<'a, str>) -> Option<PackageSource> {
    match resolved {
        // XXX: The registry can be overriden via config. https://docs.npmjs.com/cli/v10/using-npm/config#registry
        // Also per scope (see v3_scope_registry test.)
        // npm seems to always output npmjs instead of mirrors, even if overriden.
        r if r.starts_with("https://registry.npmjs.org/") => Some(PackageSource::Npm),

        r if r.starts_with("git+") => Some(PackageSource::Git { url: r.to_string() }),

        _ => None,
    }
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
    let mut pkg = PackageBuilder::new(
        name.map(PackageName::new).transpose()?,
        tree_package.version.as_ref().map(|s| s.to_string()),
    );
    if let Some(integrity) = &tree_package.integrity {
        pkg.integrity(integrity.parse()?);
    }
    if let Some(resolved) = &tree_package.resolved {
        if let Some(source) = recognize_source(resolved) {
            pkg.source(source);
        }
    }
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
        dependencies.push(
            DependencyBuilder::new(
                DependencyKind::Dependency,
                self_pid,
                find_pid(path, n, path_pid)?,
            )
            .build(),
        );
    }
    for n in tree_package.dev_dependencies.keys() {
        dependencies.push(
            DependencyBuilder::new(
                DependencyKind::DevDependency,
                self_pid,
                find_pid(path, n, path_pid)?,
            )
            .build(),
        );
    }
    for n in tree_package.peer_dependencies.keys() {
        let is_optional = match tree_package.peer_dependencies_meta.get(n) {
            Some(PeerDependencyMeta {
                optional: Some(true),
            }) => true,
            _ => false,
        };
        match find_pid(path, n, path_pid) {
            Ok(pid) => dependencies.push(
                DependencyBuilder::new(
                    if is_optional {
                        DependencyKind::OptionalPeerDependency
                    } else {
                        DependencyKind::PeerDependency
                    },
                    self_pid,
                    pid,
                )
                .build(),
            ),
            // It's optional, ignore.
            Err(Error::DependencyNotFound(_)) if is_optional => {}

            Err(e) => return Err(e),
        }
    }
    for n in tree_package.optional_dependencies.keys() {
        match find_pid(path, n, path_pid) {
            Ok(pid) => dependencies.push(
                DependencyBuilder::new(DependencyKind::OptionalDependency, self_pid, pid).build(),
            ),
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
                package.name(Some(PackageName::new(self.package_lock.name.to_string())?));
            }
            let pid = self.chastefile_builder.add_package(package.build()?);
            self.path_pid.insert(package_path, pid);
            let installation = InstallationBuilder::new(pid, package_path.to_string()).build()?;
            self.chastefile_builder
                .add_package_installation(installation);
            if package_path == "" {
                self.chastefile_builder.set_root_package_id(pid)?;
            }
        }
        for (package_path, tree_package) in self.package_lock.packages.iter() {
            let pid = self.path_pid.get(package_path).unwrap().clone();
            let dependencies = parse_dependencies(package_path, tree_package, &self.path_pid, pid)?;
            self.chastefile_builder
                .add_dependencies(dependencies.into_iter());
        }
        Ok(self.chastefile_builder.build()?)
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

/// Discouraged over [from_str] and [from_slice] as it clones all the strings.
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

    use chaste_types::{Chastefile, PackageSourceType};

    use super::{from_str, Result};

    fn test_workspace(name: &str) -> Result<Chastefile> {
        let package_lock_json =
            fs::read_to_string(format!("test_workspaces/{name}/package-lock.json"))?;
        from_str(&package_lock_json)
    }

    #[test]
    fn v3_basic() -> Result<()> {
        let chastefile = test_workspace("v3_basic")?;
        let root = chastefile.root_package();
        assert_eq!(root.name().unwrap(), "@chastelock/test__v3_basic");
        assert_eq!(root.version().unwrap().to_string(), "0.0.0");
        assert_eq!(chastefile.packages().len(), 9);
        assert_eq!(
            chastefile
                .recursive_package_dependencies(chastefile.root_package_id())
                .len(),
            8
        );

        Ok(())
    }

    #[test]
    fn v3_git_url() -> Result<()> {
        let chastefile = test_workspace("v3_git_url")?;
        let root_dev_deps: Vec<_> = chastefile
            .root_package_dependencies()
            .into_iter()
            .filter(|d| d.kind.is_dev())
            .collect();
        assert_eq!(root_dev_deps.len(), 1);
        let minimatch_dep = root_dev_deps.first().unwrap();
        let minimatch = chastefile.package(minimatch_dep.on);
        assert_eq!(minimatch.name().unwrap(), "minimatch");
        assert_eq!(minimatch.source_type(), Some(PackageSourceType::Git));

        Ok(())
    }

    #[test]
    fn v3_github_ref() -> Result<()> {
        let chastefile = test_workspace("v3_github_ref")?;
        let root_dev_deps: Vec<_> = chastefile
            .root_package_dependencies()
            .into_iter()
            .filter(|d| d.kind.is_dev())
            .collect();
        let minimatch_dep = root_dev_deps.first().unwrap();
        let minimatch = chastefile.package(minimatch_dep.on);
        assert_eq!(minimatch.name().unwrap(), "minimatch");
        assert_eq!(minimatch.source_type(), Some(PackageSourceType::Git));

        Ok(())
    }

    #[test]
    fn v3_scope_registry() -> Result<()> {
        let chastefile = test_workspace("v3_scope_registry")?;
        let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
        let empty_pkg = chastefile.package(empty_pid);
        assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
        assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
        assert_eq!(empty_pkg.integrity().hashes.len(), 1);
        // TODO: recognize custom npm registry.
        assert_eq!(empty_pkg.source_type(), None);

        Ok(())
    }
}
