// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::Path;
use std::{fs, str};

use chaste_types::Chastefile;
use yarn_lock_parser as yarn;

pub use crate::error::{Error, Result};

mod berry;
mod classic;
mod error;
mod types;

pub static LOCKFILE_NAME: &str = "yarn.lock";

pub fn parse<P>(root_dir: P) -> Result<Chastefile>
where
    P: AsRef<Path>,
{
    let root_dir = root_dir.as_ref();

    let lockfile_contents = fs::read_to_string(root_dir.join(LOCKFILE_NAME))?;
    let yarn_lock: yarn::Lockfile = yarn::parse_str(&lockfile_contents)?;

    match yarn_lock.version {
        1 => classic::resolve(yarn_lock, root_dir),
        2..=8 => berry::resolve(yarn_lock, root_dir),
        _ => Err(Error::UnknownLockfileVersion(yarn_lock.version)),
    }
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
    fn v4_basic() -> Result<()> {
        let chastefile = test_workspace("v4_basic")?;
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
    fn v4_git_ssh() -> Result<()> {
        let chastefile = test_workspace("v4_git_ssh")?;
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
        // TODO: fix
        // assert_eq!(semver.source_type(), Some(PackageSourceType::Git));
        // assert_eq!(semver.integrity().hashes.len(), 0);
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
    fn v4_git_url() -> Result<()> {
        let chastefile = test_workspace("v4_git_url")?;
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
        // TODO: fix
        // assert_eq!(minimatch.source_type(), Some(PackageSourceType::Git));
        // assert_eq!(minimatch.integrity().hashes.len(), 0);
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
    fn v4_github_ref() -> Result<()> {
        let chastefile = test_workspace("v4_github_ref")?;
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
        // TODO: fix
        // assert_eq!(package.source_type(), Some(PackageSourceType::Git));
        // assert_eq!(package.integrity().hashes.len(), 0);

        let package = root_dep_packages[1];
        assert_eq!(package.name().unwrap(), "node-semver");
        assert_eq!(package.version().unwrap().to_string(), "7.6.3");
        // TODO: fix
        // assert_eq!(package.source_type(), Some(PackageSourceType::Git));
        // assert_eq!(package.integrity().hashes.len(), 0);

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
    fn v4_scope_registry() -> Result<()> {
        let chastefile = test_workspace("v4_scope_registry")?;
        let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
        let empty_pkg = chastefile.package(empty_pid);
        assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
        assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
        assert_eq!(empty_pkg.integrity().hashes.len(), 1);
        assert_eq!(empty_pkg.source_type(), Some(PackageSourceType::Npm));

        Ok(())
    }

    #[test]
    fn v1_workspace_basic() -> Result<()> {
        let chastefile = test_workspace("v1_workspace_basic")?;
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

    #[test]
    fn v4_workspace_basic() -> Result<()> {
        let chastefile = test_workspace("v4_workspace_basic")?;
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
