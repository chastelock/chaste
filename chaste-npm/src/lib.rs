// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chaste_types::{
    Chastefile, ChastefileBuilder, Dependency, DependencyBuilder, DependencyKind,
    InstallationBuilder, PackageBuilder, PackageID, PackageName, PackageSource,
    SourceVersionDescriptor,
};

pub use crate::error::{Error, Result};
pub use crate::parsers::PathLexingError;

use crate::parsers::package_name_from_path;
use crate::types::{DependencyTreePackage, PackageLock, PeerDependencyMeta};

mod error;
mod parsers;
mod types;

pub static LOCKFILE_NAME: &str = "package-lock.json";

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

fn parse_package(path: &str, tree_package: &DependencyTreePackage) -> Result<PackageBuilder> {
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
    for (n, svd) in tree_package.dependencies.iter() {
        let mut dep = DependencyBuilder::new(
            DependencyKind::Dependency,
            self_pid,
            find_pid(path, n, path_pid)?,
        );
        dep.svd(SourceVersionDescriptor::new(svd.to_string())?);
        dependencies.push(dep.build());
    }
    for (n, svd) in tree_package.dev_dependencies.iter() {
        let mut dep = DependencyBuilder::new(
            DependencyKind::DevDependency,
            self_pid,
            find_pid(path, n, path_pid)?,
        );
        dep.svd(SourceVersionDescriptor::new(svd.to_string())?);
        dependencies.push(dep.build());
    }
    for (n, svd) in tree_package.peer_dependencies.iter() {
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
                dep.svd(SourceVersionDescriptor::new(svd.to_string())?);
                dependencies.push(dep.build());
            }
            // Allowed to fail. Yes, even if not marked as optional - it wasn't getting installed
            // before npm v7, and packages can opt out with --legacy-peer-deps=true
            // https://github.com/npm/rfcs/blob/main/implemented/0025-install-peer-deps.md
            Err(Error::DependencyNotFound(_)) => {}

            Err(e) => return Err(e),
        }
    }
    for (n, svd) in tree_package.optional_dependencies.iter() {
        match find_pid(path, n, path_pid) {
            Ok(pid) => {
                let mut dep =
                    DependencyBuilder::new(DependencyKind::OptionalDependency, self_pid, pid);
                dep.svd(SourceVersionDescriptor::new(svd.to_string())?);
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
            let mut package = parse_package(package_path, tree_package)?;
            if package_path == "" && package.get_name().is_none() {
                package.name(Some(PackageName::new(self.package_lock.name.to_string())?));
            }
            let pid = match self.chastefile_builder.add_package(package.build()?) {
                Ok(pid) => pid,
                // If the package is already checked in, reuse it.
                Err(chaste_types::Error::DuplicatePackage(pid)) => pid,
                Err(e) => return Err(Error::ChasteError(e)),
            };
            self.path_pid.insert(package_path, pid);
            let installation = InstallationBuilder::new(pid, package_path.to_string()).build()?;
            self.chastefile_builder
                .add_package_installation(installation);
            if package_path == "" {
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
            let installation = InstallationBuilder::new(pid, package_path.to_string()).build()?;
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
    if package_lock.lockfile_version != 3 {
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
    let lockfile_contents = fs::read_to_string(root_dir.as_ref().join(LOCKFILE_NAME))?;
    let package_lock: PackageLock = serde_json::from_str(&lockfile_contents)?;
    parse_lock(&package_lock)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::LazyLock;

    use chaste_types::{Chastefile, Package, PackageID, PackageSourceType};

    use super::{parse, Result};

    static TEST_WORKSPACES: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("test_workspaces"));

    fn test_workspace(name: &str) -> Result<Chastefile> {
        parse(TEST_WORKSPACES.join(name))
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
    fn v3_git_ssh() -> Result<()> {
        let chastefile = test_workspace("v3_git_ssh")?;
        let root_deps: Vec<_> = chastefile.root_package_dependencies().into_iter().collect();
        assert_eq!(root_deps.len(), 1);
        let semver_dep = root_deps.first().unwrap();
        let svd = semver_dep.svd().unwrap();
        assert!(svd.is_git());
        assert_eq!(svd.ssh_path_sep(), Some(":"));
        let semver = chastefile.package(semver_dep.on);
        assert_eq!(semver.name().unwrap(), "semver");
        assert_eq!(semver.source_type(), Some(PackageSourceType::Git));

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
        assert_eq!(minimatch.integrity().hashes.len(), 0);

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
        assert_eq!(minimatch.integrity().hashes.len(), 0);

        Ok(())
    }

    #[test]
    fn v3_hoist_partial() -> Result<()> {
        let chastefile = test_workspace("v3_hoist_partial")?;
        let mut chalks: Vec<&Package> = chastefile
            .packages()
            .into_iter()
            .filter(|p| p.name().is_some_and(|n| n == "chalk"))
            .collect();
        chalks.sort_unstable_by_key(|p| p.version());
        let [chalk2, chalk5] = *chalks else { panic!() };
        assert_eq!(chalk2.version().unwrap().to_string(), "2.4.2");
        assert_eq!(chalk5.version().unwrap().to_string(), "5.4.0");

        Ok(())
    }

    #[test]
    fn v3_npm_tag() -> Result<()> {
        let chastefile = test_workspace("v3_npm_tag")?;
        let [nop_dep] = *chastefile.root_package_dependencies() else {
            panic!();
        };
        let nop = chastefile.package(nop_dep.on);
        assert_eq!(nop.name().unwrap(), "nop");
        assert!(nop_dep.svd().unwrap().is_npm_tag());

        Ok(())
    }

    #[test]
    fn v3_peer_unsatisfied() -> Result<()> {
        let chastefile = test_workspace("v3_peer_unsatisfied")?;
        assert!(!chastefile.packages().into_iter().any(|p| p
            .name()
            .is_some_and(|n| n == "@bazel/bazelisk"
                || n == "@bazel/concatjs"
                || n == "typescript")));

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

    #[test]
    fn v3_workspace_basic() -> Result<()> {
        let chastefile = test_workspace("v3_workspace_basic")?;
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
        assert_eq!(balls_installations.len(), 2);
        let mut balls_install_paths = balls_installations
            .iter()
            .map(|i| i.path())
            .collect::<Vec<&str>>();
        balls_install_paths.sort_unstable();
        assert_eq!(
            balls_install_paths,
            ["balls", "node_modules/@chastelock/balls"]
        );

        Ok(())
    }
}
