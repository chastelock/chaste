// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};

use chaste_types::{
    Chastefile, ChastefileBuilder, Checksums, Dependency, DependencyBuilder, DependencyKind,
    InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageID, PackageName,
    PackageSource, SourceVersionSpecifier,
};

pub use crate::error::{Error, Result};

use crate::types::{DependencyTreePackage, PeerDependencyMeta};

#[cfg(feature = "fuzzing")]
pub use crate::types::PackageLock;
#[cfg(not(feature = "fuzzing"))]
use crate::types::PackageLock;

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
        // `registry.npmjs.org` is a special value that by default means the default registry[1],
        // even though the actually used registry can be overriden in the user config[2].
        // npm can also be configured to output the actual registry host[3].
        //
        // If the package name has a @scope, and the scope is configured to another registry,
        // it always is the actual registry. (See the v3_scope_registry test for an example.)
        //
        // [1]: https://docs.npmjs.com/cli/v11/configuring-npm/package-lock-json#packages
        // [2]: https://docs.npmjs.com/cli/v11/using-npm/config#registry
        // [3]: https://docs.npmjs.com/cli/v11/using-npm/config#replace-registry-host
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
        let inte: Integrity = integrity.parse()?;
        if !inte.hashes.is_empty() {
            pkg.checksums(Checksums::Tarball(inte));
        }
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
    let capacity = tree_package.dependencies.len() + tree_package.dev_dependencies.len();
    let mut dependencies = Vec::with_capacity(capacity);
    for (deps, kind_) in [
        (&tree_package.dependencies, DependencyKind::Dependency),
        (
            &tree_package.dev_dependencies,
            DependencyKind::DevDependency,
        ),
        (
            &tree_package.peer_dependencies,
            DependencyKind::PeerDependency,
        ),
        (
            &tree_package.optional_dependencies,
            DependencyKind::OptionalDependency,
        ),
    ] {
        for (n, svs) in deps {
            let kind = match kind_ {
                DependencyKind::PeerDependency
                    if matches!(
                        tree_package.peer_dependencies_meta.get(n),
                        Some(PeerDependencyMeta {
                            optional: Some(true),
                        })
                    ) =>
                {
                    DependencyKind::OptionalPeerDependency
                }
                k => k,
            };
            match find_pid(path, n, path_pid) {
                Ok(pid) => {
                    let mut dep = DependencyBuilder::new(kind, self_pid, pid);
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
                Err(Error::DependencyNotFound(_)) if kind.is_peer() || kind.is_optional() => {}

                Err(e) => return Err(e),
            }
        }
    }

    debug_assert!(dependencies.len() >= capacity);

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
            let Some(&pid) = tree_package
                .resolved
                .as_ref()
                .and_then(|lt| self.path_pid.get(lt))
            else {
                return Err(Error::WorkspaceMemberNotFound(package_path.to_string()));
            };
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

mod parse_lock_ {
    use super::{Chastefile, Error, PackageLock, PackageParser, Result};
    pub fn parse_lock(package_lock: &PackageLock) -> Result<Chastefile> {
        if ![2, 3].contains(&package_lock.lockfile_version) {
            return Err(Error::UnknownLockVersion(package_lock.lockfile_version));
        }
        let parser = PackageParser::new(package_lock);
        let chastefile = parser.resolve()?;
        Ok(chastefile)
    }
}

#[cfg(feature = "fuzzing")]
pub use parse_lock_::parse_lock;
#[cfg(not(feature = "fuzzing"))]
use parse_lock_::parse_lock;

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
