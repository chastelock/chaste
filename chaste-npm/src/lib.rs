// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};

use chaste_types::{
    Chastefile, ChastefileBuilder, Dependency, DependencyBuilder, DependencyKind,
    InstallationBuilder, ModulePath, PackageBuilder, PackageID, PackageName, PackageSource,
    SourceVersionSpecifier,
};

pub use crate::error::{Error, Result};

use crate::types::{DependencyTreePackage, PackageLock, PeerDependencyMeta};

mod error;
#[cfg(test)]
mod tests;
mod types;

pub static LOCKFILE_NAME: &str = "package-lock.json";
pub static SHRINKWRAP_NAME: &str = "npm-shrinkwrap.json";

struct PackageParser<'a> {
    package_lock: &'a PackageLock<'a>,
    chastefile_builder: ChastefileBuilder,
    path_pid: HashMap<&'a Cow<'a, str>, PackageID>,
}

fn recognize_source(resolved: &str) -> Option<PackageSource> {
    match resolved {
        // XXX: The registry can be overriden via config. https://docs.npmjs.com/cli/v10/using-npm/config#registry
        // Also per scope (see v3_scope_registry test.)
        // npm seems to always output npmjs instead of mirrors, even if overriden.
        r if r.starts_with("https://registry.npmjs.org/") => Some(PackageSource::Npm),

        r if r.starts_with("git+") => Some(PackageSource::Git { url: r.to_string() }),

        _ => None,
    }
}

fn parse_package(
    path: &ModulePath,
    tree_package: &DependencyTreePackage,
) -> Result<PackageBuilder> {
    let mut name = tree_package
        .name
        .as_ref()
        .map(|s| PackageName::new(s.to_string()))
        .transpose()?;
    // Most packages don't have it as it's implied by the path.
    // So now we have to unimply it.
    if name.is_none() {
        name = path.implied_package_name();
    }
    let mut pkg = PackageBuilder::new(name, tree_package.version.as_ref().map(|s| s.to_string()));
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
        return Ok(*pid);
    }
    if let Some((parent_path, _)) = path.rsplit_once('/') {
        return find_pid(parent_path, name, path_pid);
    }
    if !path.is_empty() {
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
    for (n, svs) in tree_package.dependencies.iter() {
        let mut dep = DependencyBuilder::new(
            DependencyKind::Dependency,
            self_pid,
            find_pid(path, n, path_pid)?,
        );
        let svs = SourceVersionSpecifier::new(svs.to_string())?;
        if svs.aliased_package_name().is_some() {
            dep.alias_name(PackageName::new(n.to_string())?);
        }
        dep.svs(svs);
        dependencies.push(dep.build());
    }
    for (n, svs) in tree_package.dev_dependencies.iter() {
        let mut dep = DependencyBuilder::new(
            DependencyKind::DevDependency,
            self_pid,
            find_pid(path, n, path_pid)?,
        );
        let svs = SourceVersionSpecifier::new(svs.to_string())?;
        if svs.aliased_package_name().is_some() {
            dep.alias_name(PackageName::new(n.to_string())?);
        }
        dep.svs(svs);
        dependencies.push(dep.build());
    }
    for (n, svs) in tree_package.peer_dependencies.iter() {
        let is_optional = matches!(
            tree_package.peer_dependencies_meta.get(n),
            Some(PeerDependencyMeta {
                optional: Some(true),
            })
        );
        match find_pid(path, n, path_pid) {
            Ok(pid) => {
                let mut dep = DependencyBuilder::new(
                    if is_optional {
                        DependencyKind::OptionalPeerDependency
                    } else {
                        DependencyKind::PeerDependency
                    },
                    self_pid,
                    pid,
                );
                let svs = SourceVersionSpecifier::new(svs.to_string())?;
                if svs.aliased_package_name().is_some() {
                    dep.alias_name(PackageName::new(n.to_string())?);
                }
                dep.svs(svs);
                dependencies.push(dep.build());
            }
            // Allowed to fail. Yes, even if not marked as optional - it wasn't getting installed
            // before npm v7, and packages can opt out with --legacy-peer-deps=true
            // https://github.com/npm/rfcs/blob/main/implemented/0025-install-peer-deps.md
            Err(Error::DependencyNotFound(_)) => {}

            Err(e) => return Err(e),
        }
    }
    for (n, svs) in tree_package.optional_dependencies.iter() {
        match find_pid(path, n, path_pid) {
            Ok(pid) => {
                let mut dep =
                    DependencyBuilder::new(DependencyKind::OptionalDependency, self_pid, pid);
                let svs = SourceVersionSpecifier::new(svs.to_string())?;
                if svs.aliased_package_name().is_some() {
                    dep.alias_name(PackageName::new(n.to_string())?);
                }
                dep.svs(svs);
                dependencies.push(dep.build());
            }
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
        // First, go through all packages, but ignore entries that say to link to another package.
        // We have to do that before we can resolve the links to their respective packages.
        for (package_path, tree_package) in self
            .package_lock
            .packages
            .iter()
            .filter(|(_, tp)| tp.link != Some(true))
        {
            let module_path = ModulePath::new(package_path.to_string())?;
            let mut package = parse_package(&module_path, tree_package)?;
            if package_path.is_empty() && package.get_name().is_none() {
                package.name(Some(PackageName::new(self.package_lock.name.to_string())?));
            }
            let pid = match self.chastefile_builder.add_package(package.build()?) {
                Ok(pid) => pid,
                // If the package is already checked in, reuse it.
                Err(chaste_types::Error::DuplicatePackage(pid)) => pid,
                Err(e) => return Err(Error::ChasteError(e)),
            };
            self.path_pid.insert(package_path, pid);
            let installation = InstallationBuilder::new(pid, module_path).build()?;
            self.chastefile_builder
                .add_package_installation(installation);
            if package_path.is_empty() {
                self.chastefile_builder.set_root_package_id(pid)?;

            // XXX: This is hacky
            } else if !package_path.starts_with("node_modules/")
                && !package_path.contains("/node_modules/")
            {
                self.chastefile_builder.set_as_workspace_member(pid)?;
            }
        }
        // Resolve the links.
        for (package_path, tree_package) in self
            .package_lock
            .packages
            .iter()
            .filter(|(_, tp)| tp.link == Some(true))
        {
            let Some(member_path) = &tree_package.resolved else {
                return Err(Error::WorkspaceMemberNotFound(package_path.to_string()));
            };
            let pid = *self.path_pid.get(member_path).unwrap();
            self.path_pid.insert(package_path, pid);
            let module_path = ModulePath::new(package_path.to_string())?;
            let installation = InstallationBuilder::new(pid, module_path).build()?;
            self.chastefile_builder
                .add_package_installation(installation);
        }
        // Now, resolve package dependencies.
        for (package_path, tree_package) in self
            .package_lock
            .packages
            .iter()
            .filter(|(_, tp)| tp.link != Some(true))
        {
            let pid = *self.path_pid.get(package_path).unwrap();
            let dependencies = parse_dependencies(package_path, tree_package, &self.path_pid, pid)?;
            self.chastefile_builder
                .add_dependencies(dependencies.into_iter());
        }
        Ok(self.chastefile_builder.build()?)
    }
}

fn parse_lock(package_lock: &PackageLock) -> Result<Chastefile> {
    if ![2, 3].contains(&package_lock.lockfile_version) {
        return Err(Error::UnknownLockVersion(package_lock.lockfile_version));
    }
    let parser = PackageParser::new(package_lock);
    let chastefile = parser.resolve()?;
    Ok(chastefile)
}

pub fn parse<P>(root_dir: P) -> Result<Chastefile>
where
    P: AsRef<Path>,
{
    let lockfile_contents = match fs::read_to_string(root_dir.as_ref().join(SHRINKWRAP_NAME)) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            fs::read_to_string(root_dir.as_ref().join(LOCKFILE_NAME))?
        }
        Err(e) => return Err(Error::IoError(e)),
    };
    let package_lock: PackageLock = serde_json::from_str(&lockfile_contents)?;
    parse_lock(&package_lock)
}
