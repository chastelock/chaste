// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::PathBuf;
use std::sync::LazyLock;

use chaste_types::{Chastefile, Package, PackageID, PackageSourceType};

use super::{parse, Error, Result};

static TEST_WORKSPACES: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("test_workspaces"));

fn test_workspace(name: &str) -> Result<Chastefile> {
    parse(TEST_WORKSPACES.join(name))
}

#[test]
fn v1_basic() -> Result<()> {
    assert!(matches!(
        test_workspace("v1_basic").unwrap_err(),
        Error::UnknownLockVersion(1)
    ));

    Ok(())
}

#[test]
fn v2_basic() -> Result<()> {
    let chastefile = test_workspace("v2_basic")?;
    let root = chastefile.root_package();
    assert_eq!(root.name().unwrap(), "@chastelock/test__v2_basic");
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
    let svs = semver_dep.svs().unwrap();
    assert!(svs.is_git());
    assert_eq!(svs.ssh_path_sep(), Some(":"));
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
    assert!(minimatch.checksums().is_none());

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
    assert!(minimatch.checksums().is_none());

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
fn v3_npm_aliased() -> Result<()> {
    let chastefile = test_workspace("v3_npm_aliased")?;
    let [pakig_dep] = *chastefile.root_package_dependencies() else {
        panic!()
    };
    assert_eq!(pakig_dep.alias_name().unwrap(), "pakig");
    assert_eq!(
        pakig_dep.svs().unwrap().aliased_package_name().unwrap(),
        "nop"
    );
    let pakig = chastefile.package(pakig_dep.on);
    assert_eq!(pakig.name().unwrap(), "nop");
    assert_eq!(pakig.version().unwrap().to_string(), "1.0.0");
    assert_eq!(pakig.checksums().unwrap().integrity().hashes.len(), 1);
    assert_eq!(pakig.source_type(), Some(PackageSourceType::Npm));

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
    assert!(nop_dep.svs().unwrap().is_npm_tag());

    Ok(())
}

#[test]
fn v3_overrides() -> Result<()> {
    let chastefile = test_workspace("v3_overrides")?;
    let [(ms_pid, ms_pkg)] = *chastefile
        .packages_with_ids()
        .into_iter()
        .filter(|(_pid, p)| p.name().is_some_and(|n| n == "ms"))
        .collect::<Vec<(PackageID, &Package)>>()
    else {
        panic!();
    };
    assert_eq!(ms_pkg.version().unwrap().to_string(), "2.1.3");
    assert_eq!(ms_pkg.source_type(), Some(PackageSourceType::Npm));
    let mut ms_svss = chastefile
        .package_dependents(ms_pid)
        .into_iter()
        .map(|d| d.svs().unwrap().as_ref())
        .collect::<Vec<&str>>();
    ms_svss.sort_unstable();
    assert_eq!(ms_svss, ["2.0.0", "2.1.3", "^2.1"]);

    let [(path_pid, path_pkg)] = *chastefile
        .packages_with_ids()
        .into_iter()
        .filter(|(_pid, p)| p.name().is_some_and(|n| n == "path-to-regexp"))
        .collect::<Vec<(PackageID, &Package)>>()
    else {
        panic!();
    };
    assert_eq!(path_pkg.version().unwrap().to_string(), "0.1.12");
    assert_eq!(path_pkg.source_type(), Some(PackageSourceType::Npm));
    let path_svss = chastefile
        .package_dependents(path_pid)
        .into_iter()
        .map(|d| d.svs().unwrap().as_ref())
        .collect::<Vec<&str>>();
    assert_eq!(path_svss, ["0.1.10"]);

    let [(scwm_pid, scwm_pkg)] = *chastefile
        .packages_with_ids()
        .into_iter()
        .filter(|(_pid, p)| p.name().is_some_and(|n| n == "side-channel-weakmap"))
        .collect::<Vec<(PackageID, &Package)>>()
    else {
        panic!();
    };
    assert_eq!(scwm_pkg.version().unwrap().to_string(), "1.0.1");
    // TODO: Recognize as tarball
    assert_eq!(scwm_pkg.source_type(), None);
    let scwm_svss = chastefile
        .package_dependents(scwm_pid)
        .into_iter()
        .map(|d| d.svs().unwrap().as_ref())
        .collect::<Vec<&str>>();
    assert_eq!(scwm_svss, ["^1.0.2"]);

    Ok(())
}

#[test]
fn v3_peer_unsatisfied() -> Result<()> {
    let chastefile = test_workspace("v3_peer_unsatisfied")?;
    assert!(!chastefile.packages().into_iter().any(|p| p
        .name()
        .is_some_and(|n| n == "@bazel/bazelisk" || n == "@bazel/concatjs" || n == "typescript")));

    Ok(())
}

#[test]
fn v3_scope_registry() -> Result<()> {
    let chastefile = test_workspace("v3_scope_registry")?;
    let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
    let empty_pkg = chastefile.package(empty_pid);
    assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
    assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
    assert_eq!(empty_pkg.checksums().unwrap().integrity().hashes.len(), 1);
    // TODO: recognize custom npm registry.
    assert_eq!(empty_pkg.source_type(), None);

    Ok(())
}

#[test]
fn v3_shrinkwrap() -> Result<()> {
    let chastefile = test_workspace("v3_shrinkwrap")?;
    let root = chastefile.root_package();
    assert_eq!(root.name().unwrap(), "@chastelock/testcase");
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
fn v3_tarball_url() -> Result<()> {
    let chastefile = test_workspace("v3_tarball_url")?;
    let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
    let empty_pkg = chastefile.package(empty_pid);
    assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
    assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
    assert_eq!(empty_pkg.checksums().unwrap().integrity().hashes.len(), 1);
    // TODO: recognize as arbitrary tarball
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
    assert!(workspace_member_ids.contains(&balls_pid) && workspace_member_ids.contains(&ligma_pid));
    let balls_installations = chastefile.package_installations(balls_pid);
    assert_eq!(balls_installations.len(), 2);
    let mut balls_install_paths = balls_installations
        .iter()
        .map(|i| i.path().as_ref())
        .collect::<Vec<&str>>();
    balls_install_paths.sort_unstable();
    assert_eq!(
        balls_install_paths,
        ["balls", "node_modules/@chastelock/balls"]
    );

    Ok(())
}
