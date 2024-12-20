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
    use concat_idents::concat_idents;

    use super::{parse, Result};

    static TEST_WORKSPACES: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("test_workspaces"));

    macro_rules! test_workspace {
        ([$v:expr], $name:ident, $solver:expr) => {
            concat_idents!(fn_name = v, $v, _, $name {
                #[test]
                fn fn_name() -> Result<()> {
                    ($solver)(parse(TEST_WORKSPACES.join(format!("v{}_{}", $v, stringify!($name))))?, $v)
                }
            });
        };
        ([$v:expr, $($vothers:expr),+], $name:ident, $solver:expr) => {
            test_workspace!([$v], $name, $solver);
            test_workspace!([$($vothers),+], $name, $solver);
        };
    }
    macro_rules! test_workspaces {
        ($name:ident, $solver:expr) => {
            test_workspace!([1, 4, 6, 8], $name, $solver);
        };
    }

    test_workspaces!(basic, |chastefile: Chastefile, lv: u8| {
        let rec_deps = chastefile.recursive_package_dependencies(chastefile.root_package_id());
        assert_eq!(rec_deps.len(), 5);
        assert!(rec_deps
            .iter()
            .map(|d| chastefile.package(d.on))
            .all(|p| p.source_type() == Some(PackageSourceType::Npm)));
        Ok(())
    });

    test_workspaces!(git_ssh, |chastefile: Chastefile, lv: u8| {
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
        // XXX: https://codeberg.org/selfisekai/chaste/issues/23
        assert_eq!(semver.integrity().hashes.len(), if lv == 1 { 0 } else { 1 });
        Ok(())
    });

    test_workspaces!(git_url, |chastefile: Chastefile, lv: u8| {
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
        // TODO: https://codeberg.org/selfisekai/chaste/issues/25
        assert_eq!(
            minimatch.source_type(),
            if lv == 1 {
                Some(PackageSourceType::Git)
            } else {
                None
            }
        );
        // XXX: https://codeberg.org/selfisekai/chaste/issues/23
        assert_eq!(
            minimatch.integrity().hashes.len(),
            if lv == 1 { 0 } else { 1 }
        );
        Ok(())
    });

    test_workspaces!(github_ref, |chastefile: Chastefile, lv: u8| {
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
        // TODO: https://codeberg.org/selfisekai/chaste/issues/25
        assert_eq!(
            package.source_type(),
            if lv == 1 {
                Some(PackageSourceType::TarballURL)
            } else {
                None
            }
        );
        // XXX: https://codeberg.org/selfisekai/chaste/issues/23
        assert_eq!(
            package.integrity().hashes.len(),
            if lv == 1 { 0 } else { 1 }
        );

        let package = root_dep_packages[1];
        assert_eq!(package.name().unwrap(), "node-semver");
        assert_eq!(package.version().unwrap().to_string(), "7.6.3");
        assert_eq!(
            package.source_type(),
            if lv == 1 {
                Some(PackageSourceType::TarballURL)
            } else {
                None
            }
        );
        // XXX: https://codeberg.org/selfisekai/chaste/issues/23
        assert_eq!(
            package.integrity().hashes.len(),
            if lv == 1 { 0 } else { 1 }
        );

        Ok(())
    });

    test_workspaces!(npm_tag, |chastefile: Chastefile, lv: u8| {
        let [nop_dep] = *chastefile.root_package_dependencies() else {
            panic!();
        };
        let nop = chastefile.package(nop_dep.on);
        assert_eq!(nop.name().unwrap(), "nop");
        assert!(nop_dep.svd().unwrap().is_npm_tag());
        Ok(())
    });

    // TODO: Expand to berry. https://codeberg.org/selfisekai/chaste/issues/37
    test_workspace!([1], peer_unsatisfied, |chastefile: Chastefile, lv: u8| {
        assert!(!chastefile.packages().into_iter().any(|p| p
            .name()
            .is_some_and(|n| n == "@bazel/bazelisk"
                || n == "@bazel/concatjs"
                || n == "typescript")));
        Ok(())
    });

    test_workspaces!(scope_registry, |chastefile: Chastefile, lv: u8| {
        let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
        let empty_pkg = chastefile.package(empty_pid);
        assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
        assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
        assert_eq!(
            empty_pkg.integrity().hashes.len(),
            if lv == 1 { 2 } else { 1 }
        );
        assert_eq!(empty_pkg.source_type(), Some(PackageSourceType::Npm));

        Ok(())
    });

    test_workspaces!(workspace_basic, |chastefile: Chastefile, lv: u8| {
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
        let mut balls_install_paths = balls_installations
            .iter()
            .map(|i| i.path())
            .collect::<Vec<&str>>();
        balls_install_paths.sort_unstable();
        // There are 2: where the package is, and a link in "node_modules/{pkg.name}".
        // In classic, only the former is currently tracked, in berry, the latter is tracked if yarn-state is present.
        if lv == 1 {
            assert_eq!(balls_installations.len(), 1);
            assert_eq!(balls_install_paths, ["balls"]);
        } else {
            assert_eq!(balls_installations.len(), 2);
            assert_eq!(
                balls_install_paths,
                ["balls", "node_modules/@chastelock/balls"]
            );
        }

        Ok(())
    });
}
