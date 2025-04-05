// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::PathBuf;
use std::sync::LazyLock;

use chaste_types::{Chastefile, DependencyKind, Package, PackageID, PackageSourceType};

use crate::error::Result;
use crate::parse;

static TEST_WORKSPACES: LazyLock<PathBuf> = LazyLock::new(|| PathBuf::from("test_workspaces"));

fn test_workspace(name: &str) -> Result<Chastefile> {
    parse(TEST_WORKSPACES.join(name))
}

#[test]
fn v9_basic() -> Result<()> {
    let chastefile = test_workspace("v9_basic")?;
    let root = chastefile.root_package();
    assert_eq!(root.name().unwrap(), "@chastelock/test__v9_basic");
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
fn v9_git_ssh() -> Result<()> {
    let chastefile = test_workspace("v9_git_ssh")?;
    let root_deps: Vec<_> = chastefile.root_package_dependencies().into_iter().collect();
    assert_eq!(root_deps.len(), 1);
    let dep = root_deps.first().unwrap();
    let svs = dep.svs().unwrap();
    assert!(svs.is_git());
    assert_eq!(svs.ssh_path_sep(), Some("/"));
    let pkg = chastefile.package(dep.on);
    assert_eq!(pkg.name().unwrap(), "@selfisekai/gulp-sass");
    assert_eq!(pkg.source_type(), Some(PackageSourceType::Git));

    Ok(())
}

#[test]
fn v9_git_url() -> Result<()> {
    let chastefile = test_workspace("v9_git_url")?;
    let root_dev_deps: Vec<_> = chastefile
        .root_package_dependencies()
        .into_iter()
        .filter(|d| d.kind.is_dev())
        .collect();
    assert_eq!(root_dev_deps.len(), 1);
    let doipjs_dep = root_dev_deps.first().unwrap();
    let doipjs = chastefile.package(doipjs_dep.on);
    assert_eq!(doipjs.name().unwrap(), "doipjs");
    assert_eq!(doipjs.source_type(), Some(PackageSourceType::Git));
    assert!(doipjs.checksums().is_none());

    Ok(())
}

#[test]
fn v9_github_ref() -> Result<()> {
    let chastefile = test_workspace("v9_github_ref")?;
    let root_dev_deps: Vec<_> = chastefile
        .root_package_dependencies()
        .into_iter()
        .filter(|d| d.kind.is_dev())
        .collect();
    let minimatch_dep = root_dev_deps.first().unwrap();
    let minimatch = chastefile.package(minimatch_dep.on);
    assert_eq!(minimatch.name().unwrap(), "minimatch");
    assert_eq!(minimatch.source_type(), Some(PackageSourceType::TarballURL));
    assert!(minimatch.checksums().is_none());

    Ok(())
}

#[test]
fn v9_hoist_partial() -> Result<()> {
    let chastefile = test_workspace("v9_hoist_partial")?;
    let mut chalks: Vec<&Package> = chastefile
        .packages()
        .into_iter()
        .filter(|p| p.name().is_some_and(|n| n == "chalk"))
        .collect();
    chalks.sort_unstable_by_key(|p| p.version());
    let [chalk2, chalk5] = *chalks else { panic!() };
    assert_eq!(chalk2.version().unwrap().to_string(), "2.4.2");
    assert_eq!(chalk5.version().unwrap().to_string(), "5.4.1");

    Ok(())
}

#[test]
fn v9_npm_aliased() -> Result<()> {
    let chastefile = test_workspace("v9_npm_aliased")?;
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
fn v9_npm_tag() -> Result<()> {
    let chastefile = test_workspace("v9_npm_tag")?;
    let [nop_dep] = *chastefile.root_package_dependencies() else {
        panic!();
    };
    let nop = chastefile.package(nop_dep.on);
    assert_eq!(nop.name().unwrap(), "nop");
    assert!(nop_dep.svs().unwrap().is_npm_tag());

    Ok(())
}

#[test]
fn v9_overrides() -> Result<()> {
    let chastefile = test_workspace("v9_overrides")?;
    let [(_ms_pid, ms_pkg)] = *chastefile
        .packages_with_ids()
        .into_iter()
        .filter(|(_pid, p)| p.name().is_some_and(|n| n == "ms"))
        .collect::<Vec<(PackageID, &Package)>>()
    else {
        panic!();
    };
    assert_eq!(ms_pkg.version().unwrap().to_string(), "2.1.3");
    assert_eq!(ms_pkg.source_type(), Some(PackageSourceType::Npm));

    let [(_path_pid, path_pkg)] = *chastefile
        .packages_with_ids()
        .into_iter()
        .filter(|(_pid, p)| p.name().is_some_and(|n| n == "path-to-regexp"))
        .collect::<Vec<(PackageID, &Package)>>()
    else {
        panic!();
    };
    assert_eq!(path_pkg.version().unwrap().to_string(), "0.1.12");
    assert_eq!(path_pkg.source_type(), Some(PackageSourceType::Npm));

    let [(_scwm_pid, scwm_pkg)] = *chastefile
        .packages_with_ids()
        .into_iter()
        .filter(|(_pid, p)| p.name().is_some_and(|n| n == "side-channel-weakmap"))
        .collect::<Vec<(PackageID, &Package)>>()
    else {
        panic!();
    };
    assert_eq!(scwm_pkg.version().unwrap().to_string(), "1.0.1");
    assert_eq!(scwm_pkg.source_type(), Some(PackageSourceType::TarballURL));

    Ok(())
}

#[test]
fn v9_peer_circular() -> Result<()> {
    let chastefile = test_workspace("v9_peer_circular")?;

    let [circ_a_dep] = *chastefile.root_package_dependencies() else {
        panic!();
    };

    let circ_a_pid = circ_a_dep.on;
    let circ_a_pkg = chastefile.package(circ_a_pid);
    assert_eq!(circ_a_pkg.name().unwrap(), "@chastelock/circular-peers-a");

    let mut circ_a_deps: Vec<_> = chastefile
        .package_dependencies(circ_a_pid)
        .into_iter()
        .map(|d| (d, chastefile.package(d.on)))
        .collect();
    circ_a_deps.sort_unstable_by_key(|(_, p)| p.name());

    let [(circ_b_dep, circ_b_pkg), (rec_a_dep, rec_a_pkg)] = *circ_a_deps else {
        panic!();
    };

    assert_eq!(circ_b_dep.kind, DependencyKind::PeerDependency);
    assert_eq!(circ_b_pkg.name().unwrap(), "@chastelock/circular-peers-b");

    assert_eq!(rec_a_dep.kind, DependencyKind::PeerDependency);
    assert_eq!(rec_a_pkg.name().unwrap(), "@chastelock/recursion-a");

    Ok(())
}

#[test]
fn v9_peer_deps() -> Result<()> {
    let chastefile = test_workspace("v9_peer_deps")?;
    let [rrouter_dep] = *chastefile.root_package_dependencies() else {
        panic!();
    };
    assert_eq!(
        chastefile.package(rrouter_dep.on).name().unwrap(),
        "react-router"
    );
    let rrouter_deps = chastefile.package_dependencies(rrouter_dep.on).into_iter();
    assert_eq!(rrouter_deps.len(), 6);
    let (rdom_dep, _rdom_pkg) = chastefile
        .package_dependencies(rrouter_dep.on)
        .into_iter()
        .find_map(|d| {
            Some((d, chastefile.package(d.on)))
                .filter(|(_, p)| p.name().is_some_and(|n| n == "react-dom"))
        })
        .unwrap();
    assert_eq!(rdom_dep.svs().unwrap(), ">=18");
    let mut rdom_deps = chastefile.package_dependencies(rdom_dep.on).into_iter();
    assert_eq!(rdom_deps.len(), 2);
    // A dependency of react-dom here (though it's also in tree a dependency of react-router)
    let react_dep = rdom_deps.find(|d| d.kind.is_peer()).unwrap();
    assert_eq!(react_dep.svs().unwrap(), "^19.0.0");
    let react_pkg = chastefile.package(react_dep.on);
    assert_eq!(react_pkg.name().unwrap(), "react");
    /*
    let [react_inst] = *chastefile.package_installations(react_dep.on) else {
        panic!();
    };
    assert_eq!(react_inst.path().as_ref(), "node_modules/react");
    */

    Ok(())
}

#[test]
fn v9_peer_unsatisfied() -> Result<()> {
    let chastefile = test_workspace("v9_peer_unsatisfied")?;
    assert!(!chastefile.packages().into_iter().any(|p| p
        .name()
        .is_some_and(|n| n == "@bazel/bazelisk" || n == "@bazel/concatjs" || n == "typescript")));

    Ok(())
}

#[test]
fn v9_scope_registry() -> Result<()> {
    let chastefile = test_workspace("v9_scope_registry")?;
    let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
    let empty_pkg = chastefile.package(empty_pid);
    assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
    assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
    assert_eq!(empty_pkg.checksums().unwrap().integrity().hashes.len(), 1);
    assert_eq!(empty_pkg.source_type(), Some(PackageSourceType::Npm));

    Ok(())
}

#[test]
fn v9_special_chars_name() -> Result<()> {
    let chastefile = test_workspace("v9_special_chars_name")?;
    let root_pkg = chastefile.root_package();
    assert_eq!(root_pkg.name().unwrap(), "verboden(root)(name~'!*)");
    let [a_dep] = *chastefile.root_package_dependencies() else {
        panic!();
    };
    let a_pkg = chastefile.package(a_dep.on);
    assert_eq!(a_pkg.name().unwrap(), "@a/verboden(name~'!*)");
    assert_eq!(a_pkg.source_type(), Some(PackageSourceType::Npm));

    Ok(())
}

#[test]
fn v9_tarball_url() -> Result<()> {
    let chastefile = test_workspace("v9_tarball_url")?;
    let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
    let empty_pkg = chastefile.package(empty_pid);
    assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
    assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
    assert!(empty_pkg.checksums().is_none());
    assert_eq!(empty_pkg.source_type(), Some(PackageSourceType::TarballURL));

    Ok(())
}

#[test]
fn v9_workspace_basic() -> Result<()> {
    let chastefile = test_workspace("v9_workspace_basic")?;
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
    // There are 2: where the package is, and a link in "node_modules/{pkg.name}".
    // But installations to node_modules/ are currently not tracked.
    assert_eq!(balls_installations.len(), 1);
    let balls_install_paths = balls_installations
        .iter()
        .map(|i| i.path().as_ref())
        .collect::<Vec<&str>>();
    assert_eq!(balls_install_paths, ["balls"]);

    Ok(())
}
