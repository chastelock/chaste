// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::cmp::Ordering;
use std::path::PathBuf;
use std::sync::LazyLock;

use chaste_types::{
    Chastefile, Checksums, Dependency, DependencyKind, Package, PackageDerivation, PackageID,
    PackageSourceType,
};
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
        #[cfg(feature = "classic")]
        test_workspace!([1], $name, $solver);
        #[cfg(feature = "berry")]
        test_workspace!([4, 6, 8], $name, $solver);
    };
}
macro_rules! test_workspaces_berry {
    ($name:ident, $solver:expr) => {
        #[cfg(feature = "berry")]
        test_workspace!([4, 6, 8], $name, $solver);
    };
}

test_workspaces!(basic, |chastefile: Chastefile, _lv: u8| {
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
    let svs = semver_dep.svs().unwrap();
    assert!(svs.is_git());
    assert_eq!(svs.ssh_path_sep(), Some("/"));
    let semver = chastefile.package(semver_dep.on);
    assert_eq!(semver.name().unwrap(), "node-semver");
    assert_eq!(semver.version().unwrap().to_string(), "7.6.3");
    assert_eq!(semver.source_type(), Some(PackageSourceType::Git));
    if lv == 1 {
        assert!(semver.checksums().is_none());
    } else {
        let checksums = semver.checksums().unwrap();
        assert!(matches!(checksums, Checksums::RepackZip(_)));
        assert_eq!(checksums.integrity().hashes.len(), 1);
    }
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
    assert_eq!(minimatch.source_type(), Some(PackageSourceType::Git));
    if lv == 1 {
        assert!(minimatch.checksums().is_none());
    } else {
        let checksums = minimatch.checksums().unwrap();
        assert!(matches!(checksums, Checksums::RepackZip(_)));
        assert_eq!(checksums.integrity().hashes.len(), 1);
    }
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
    assert_eq!(
        package.source_type(),
        if lv == 1 {
            Some(PackageSourceType::TarballURL)
        } else {
            Some(PackageSourceType::Git)
        }
    );
    if lv == 1 {
        assert!(package.checksums().is_none());
    } else {
        let checksums = package.checksums().unwrap();
        assert!(matches!(checksums, Checksums::RepackZip(_)));
        assert_eq!(checksums.integrity().hashes.len(), 1);
    }

    let package = root_dep_packages[1];
    assert_eq!(package.name().unwrap(), "node-semver");
    assert_eq!(package.version().unwrap().to_string(), "7.6.3");
    assert_eq!(
        package.source_type(),
        if lv == 1 {
            Some(PackageSourceType::TarballURL)
        } else {
            Some(PackageSourceType::Git)
        }
    );
    if lv == 1 {
        assert!(package.checksums().is_none());
    } else {
        let checksums = package.checksums().unwrap();
        assert!(matches!(checksums, Checksums::RepackZip(_)));
        assert_eq!(checksums.integrity().hashes.len(), 1);
    }

    Ok(())
});

#[cfg(feature = "classic")]
test_workspace!(
    [1],
    npm_alias_duplicate,
    |chastefile: Chastefile, _lv: u8| {
        assert_eq!(
            chastefile
                .recursive_package_dependencies(chastefile.root_package_id())
                .len(),
            8
        );
        let root_package_dependencies = chastefile.root_package_dependencies();
        assert_eq!(root_package_dependencies.len(), 2);
        let mut root_dep_packages: Vec<(PackageID, &Package)> = root_package_dependencies
            .iter()
            .map(|d| (d.on, chastefile.package(d.on)))
            .collect();
        root_dep_packages.sort_unstable_by_key(|(_pid, p)| p.name());
        let [(event_stream_pid, event_stream_pkg), (map_stream_pid, map_stream_pkg)] =
            *root_dep_packages
        else {
            panic!()
        };
        assert_eq!(event_stream_pkg.name().unwrap(), "event-stream");
        assert_eq!(map_stream_pkg.name().unwrap(), "map-stream");
        let mut map_stream_dependents: Vec<(&Dependency, &Package)> = chastefile
            .package_dependents(map_stream_pid)
            .into_iter()
            .map(|d| (d, chastefile.package(d.from)))
            .collect();
        map_stream_dependents.sort_unstable_by_key(|(_pid, pkg)| pkg.name());
        let [(root_dep, _), (es_dep, _)] = *map_stream_dependents else {
            panic!();
        };
        assert_eq!(root_dep.from, chastefile.root_package_id());
        assert_eq!(root_dep.alias_name().unwrap(), "map");
        assert_eq!(es_dep.from, event_stream_pid);
        assert_eq!(es_dep.alias_name(), None);

        Ok(())
    }
);

test_workspaces!(npm_aliased, |chastefile: Chastefile, lv: u8| {
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
    assert_eq!(
        pakig.checksums().unwrap().integrity().hashes.len(),
        if lv == 1 { 2 } else { 1 }
    );
    assert_eq!(pakig.source_type(), Some(PackageSourceType::Npm));

    Ok(())
});

test_workspaces!(npm_tag, |chastefile: Chastefile, _lv: u8| {
    let [nop_dep] = *chastefile.root_package_dependencies() else {
        panic!();
    };
    let nop = chastefile.package(nop_dep.on);
    assert_eq!(nop.name().unwrap(), "nop");
    assert!(nop_dep.svs().unwrap().is_npm_tag());
    Ok(())
});

test_workspaces!(optional_deps, |chastefile: Chastefile, _lv: u8| {
    let [jf_dep] = *chastefile.root_package_dependencies() else {
        panic!();
    };
    let jf_pid = jf_dep.on;

    let [gfs_dep] = *chastefile.package_dependencies(jf_pid) else {
        panic!()
    };
    assert_eq!(gfs_dep.kind, DependencyKind::OptionalDependency);

    Ok(())
});

test_workspaces_berry!(patch, |chastefile: Chastefile, _lv: u8| {
    let [rec_a_dep] = *chastefile.root_package_dependencies() else {
        panic!();
    };
    let [rec_b_dep] = *chastefile.package_dependencies(rec_a_dep.on) else {
        panic!();
    };
    let rec_b_pkg = chastefile.package(rec_b_dep.on);
    assert_eq!(rec_b_pkg.name().unwrap(), "@chastelock/recursion-b");
    assert_eq!(rec_b_pkg.source(), None);

    assert!(rec_b_pkg.is_derived());
    let deriv_meta = rec_b_pkg.derivation_meta().unwrap();
    assert!(matches!(
        deriv_meta.derivation(),
        PackageDerivation::Patch(_)
    ));
    let patch = deriv_meta.patch().unwrap();
    assert_eq!(patch.path(), "patches/recursion-b.patch");
    assert_eq!(patch.integrity(), None);

    let rec_b_og_pkg = chastefile.package(deriv_meta.derived_from());
    assert!(!rec_b_og_pkg.is_derived());
    assert_eq!(rec_b_og_pkg.derivation(), None);

    Ok(())
});

test_workspaces!(
    peer_conflict_with_direct,
    |chastefile: Chastefile, _lv: u8| {
        let mut root_deps = chastefile
            .root_package_dependencies()
            .into_iter()
            .map(|d| (d, chastefile.package(d.on)))
            .collect::<Vec<_>>();
        root_deps.sort_unstable_by(|(d1, p1), (_d2, p2)| {
            p1.name().cmp(&p2.name()).then_with(|| {
                if d1.kind == DependencyKind::PeerDependency {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            })
        });
        let [(peer_mdb_dep, _peer_mdb_pkg), (regular_mdb_dep, regular_mdb_pkg), (types_dep, types_pkg)] =
            *root_deps
        else {
            panic!();
        };

        assert_eq!(peer_mdb_dep.on, regular_mdb_dep.on);
        assert_eq!(peer_mdb_dep.kind, DependencyKind::PeerDependency);
        assert_eq!(regular_mdb_dep.kind, DependencyKind::Dependency);
        assert_eq!(regular_mdb_pkg.name().unwrap(), "mime-db");
        assert_eq!(regular_mdb_pkg.version().unwrap().to_string(), "1.54.0");

        assert_eq!(types_pkg.name().unwrap(), "mime-types");

        let [indirect_mdb_dep] = *chastefile.package_dependencies(types_dep.on) else {
            panic!();
        };
        let indirect_mdb_pkg = chastefile.package(indirect_mdb_dep.on);
        assert_eq!(indirect_mdb_pkg.name().unwrap(), "mime-db");
        assert_eq!(indirect_mdb_pkg.version().unwrap().to_string(), "1.52.0");

        Ok(())
    }
);

test_workspaces!(peer_deps, |chastefile: Chastefile, lv: u8| {
    let mut root_deps: Vec<_> = chastefile
        .root_package_dependencies()
        .into_iter()
        .map(|d| (d, chastefile.package(d.on)))
        .collect();
    root_deps.sort_unstable_by_key(|(_, pkg)| pkg.name());
    let [(_, _), (_, _), (rrouter_dep, rrouter_pkg)] = *root_deps else {
        panic!();
    };
    assert_eq!(rrouter_pkg.name().unwrap(), "react-router");
    let rrouter_deps = chastefile.package_dependencies(rrouter_dep.on).into_iter();
    // v1 lockfile does not list peer dependencies at all.
    assert_eq!(rrouter_deps.len(), if lv == 1 { 4 } else { 6 });
    if lv > 1 {
        let (rdom_dep, _rdom_pkg) = chastefile
            .package_dependencies(rrouter_dep.on)
            .into_iter()
            .find_map(|d| {
                Some((d, chastefile.package(d.on)))
                    .filter(|(_, p)| p.name().is_some_and(|n| n == "react-dom"))
            })
            .unwrap();
        assert_eq!(rdom_dep.kind, DependencyKind::OptionalPeerDependency);
        assert_eq!(rdom_dep.svs().unwrap(), ">=18");
        let mut rdom_deps = chastefile.package_dependencies(rdom_dep.on).into_iter();
        assert_eq!(rdom_deps.len(), 2);
        let react_dep = rdom_deps.find(|d| d.kind.is_peer()).unwrap();
        assert_eq!(react_dep.svs().unwrap(), "^19.0.0");
        let react_pkg = chastefile.package(react_dep.on);
        assert_eq!(react_pkg.name().unwrap(), "react");
        // This requires node_modules/.yarn-state.yml
        let [react_inst] = *chastefile.package_installations(react_dep.on) else {
            panic!();
        };
        assert_eq!(react_inst.path().as_ref(), "node_modules/react");
    }

    Ok(())
});

test_workspaces!(peer_unsatisfied, |chastefile: Chastefile, _lv: u8| {
    assert!(!chastefile.packages().into_iter().any(|p| p
        .name()
        .is_some_and(|n| n == "@bazel/bazelisk" || n == "@bazel/concatjs" || n == "typescript")));
    Ok(())
});

test_workspaces!(resolutions, |chastefile: Chastefile, lv: u8| {
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
    assert_eq!(path_svss, [if lv == 8 { "npm:0.1.10" } else { "0.1.10" }]);

    let [(scwm_pid, scwm_pkg)] = *chastefile
        .packages_with_ids()
        .into_iter()
        .filter(|(_pid, p)| p.name().is_some_and(|n| n == "side-channel-weakmap"))
        .collect::<Vec<(PackageID, &Package)>>()
    else {
        panic!();
    };
    assert_eq!(scwm_pkg.version().unwrap().to_string(), "1.0.1");
    assert_eq!(
        scwm_pkg.source_type(),
        if lv == 1 {
            // TODO: Recognize as tarball
            None
        } else {
            Some(PackageSourceType::TarballURL)
        }
    );
    let scwm_svss = chastefile
        .package_dependents(scwm_pid)
        .into_iter()
        .map(|d| d.svs().unwrap().as_ref())
        .collect::<Vec<&str>>();
    assert_eq!(scwm_svss, [if lv == 8 { "npm:^1.0.2" } else { "^1.0.2" }]);

    Ok(())
});

test_workspaces!(scope_registry, |chastefile: Chastefile, lv: u8| {
    let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
    let empty_pkg = chastefile.package(empty_pid);
    assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
    assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
    assert_eq!(
        empty_pkg.checksums().unwrap().integrity().hashes.len(),
        if lv == 1 { 2 } else { 1 }
    );
    assert_eq!(empty_pkg.source_type(), Some(PackageSourceType::Npm));

    Ok(())
});

test_workspaces!(special_chars_name, |chastefile: Chastefile, _lv: u8| {
    let root_pkg = chastefile.root_package();
    assert_eq!(root_pkg.name().unwrap(), "verboden(root)(name~'!*)");
    let [a_dep] = *chastefile.root_package_dependencies() else {
        panic!();
    };
    let a_pkg = chastefile.package(a_dep.on);
    assert_eq!(a_pkg.name().unwrap(), "@a/verboden(name~'!*)");
    assert_eq!(a_pkg.source_type(), Some(PackageSourceType::Npm));

    Ok(())
});

test_workspaces!(tarball_url, |chastefile: Chastefile, _lv: u8| {
    let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
    let empty_pkg = chastefile.package(empty_pid);
    assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
    assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
    assert_eq!(empty_pkg.checksums().unwrap().integrity().hashes.len(), 1);
    assert_eq!(empty_pkg.source_type(), Some(PackageSourceType::TarballURL));

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
    assert!(workspace_member_ids.contains(&balls_pid) && workspace_member_ids.contains(&ligma_pid));
    let balls_installations = chastefile.package_installations(balls_pid);
    let mut balls_install_paths = balls_installations
        .iter()
        .map(|i| i.path().as_ref())
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

test_workspaces!(workspace_globs, |chastefile: Chastefile, lv: u8| {
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
    let mut balls_install_paths = balls_installations
        .iter()
        .map(|i| i.path().as_ref())
        .collect::<Vec<&str>>();
    balls_install_paths.sort_unstable();
    // There are 2: where the package is, and a link in "node_modules/{pkg.name}".
    // In classic, only the former is currently tracked, in berry, the latter is tracked if yarn-state is present.
    if lv == 1 {
        assert_eq!(balls_installations.len(), 1);
        assert_eq!(balls_install_paths, ["pkgs/balls"]);
    } else {
        assert_eq!(balls_installations.len(), 2);
        assert_eq!(
            balls_install_paths,
            ["node_modules/@chastelock/balls", "pkgs/balls"]
        );
    }

    Ok(())
});
