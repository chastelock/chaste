// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chaste_types::{
    Chastefile, ChastefileBuilder, DependencyBuilder, DependencyKind, InstallationBuilder,
    ModulePath, PackageBuilder, PackageName, PackageSource, SourceVersionSpecifier,
    PACKAGE_JSON_FILENAME,
};
use nom::bytes::complete::{tag, take_while1};
use nom::combinator::{opt, recognize, rest, verify};
use nom::sequence::{preceded, terminated};
use nom::{IResult, Parser};

pub use crate::error::Error;
use crate::error::Result;

mod error;
mod types;

pub static LOCKFILE_NAME: &str = "pnpm-lock.yaml";

fn package_name_part(input: &str) -> IResult<&str, &str> {
    verify(
        take_while1(|c: char| {
            c.is_ascii_alphanumeric() || c.is_ascii_digit() || ['.', '-', '_'].contains(&c)
        }),
        |part: &str| !part.starts_with("."),
    )
    .parse(input)
}

fn package_name(input: &str) -> IResult<&str, &str, nom::error::Error<&str>> {
    recognize((
        opt(preceded(tag("@"), terminated(package_name_part, tag("/")))),
        verify(package_name_part, |part: &str| {
            part != "node_modules" && part != "favicon.ico"
        }),
    ))
    .parse(input)
}

pub fn parse<P>(root_dir: P) -> Result<Chastefile>
where
    P: AsRef<Path>,
{
    let root_dir = root_dir.as_ref();

    let lockfile_contents = fs::read_to_string(root_dir.join(LOCKFILE_NAME))?;
    let lockfile: types::Lockfile = serde_norway::from_str(&lockfile_contents)?;

    let mut chastefile = ChastefileBuilder::new();

    let mut importer_to_pid = HashMap::with_capacity(lockfile.importers.len());
    for importer_path in lockfile.importers.keys() {
        let package_json_contents = fs::read_to_string(if *importer_path == "." {
            root_dir.join(PACKAGE_JSON_FILENAME)
        } else {
            root_dir.join(importer_path).join(PACKAGE_JSON_FILENAME)
        })?;
        let package_json: types::PackageJson = serde_json::from_str(&package_json_contents)?;
        let importer_pkg = PackageBuilder::new(
            package_json
                .name
                .map(|n| PackageName::new(n.to_string()))
                .transpose()?,
            package_json.version.map(|v| v.to_string()),
        );
        let importer_pid = chastefile.add_package(importer_pkg.build()?)?;
        if *importer_path == "." {
            chastefile.set_root_package_id(importer_pid)?;
        } else {
            chastefile.set_as_workspace_member(importer_pid)?;
        }
        importer_to_pid.insert(importer_path, importer_pid);
        let installation = InstallationBuilder::new(
            importer_pid,
            ModulePath::new(if *importer_path == "." {
                "".to_string()
            } else {
                importer_path.to_string()
            })?,
        )
        .build()?;
        chastefile.add_package_installation(installation);
    }

    let mut desc_pid = HashMap::with_capacity(lockfile.packages.len());
    for (pkg_desc, pkg) in lockfile.packages {
        let (_, (package_name, _, package_svs)) = (package_name, tag("@"), rest)
            .parse(pkg_desc)
            .map_err(|_| Error::InvalidPackageDescriptor(pkg_desc.to_string()))?;
        let version = pkg.version.or(Some(package_svs)).map(|v| v.to_string());
        let mut package =
            PackageBuilder::new(Some(PackageName::new(package_name.to_string())?), version);
        if let Some(integrity) = pkg.resolution.integrity {
            package.integrity(integrity.parse()?);
        }
        if let Some(tarball_url) = pkg.resolution.tarball {
            // If there is a checksum, it's a registry.
            if pkg.resolution.integrity.is_some() {
                package.source(PackageSource::Npm);
            } else {
                package.source(PackageSource::TarballURL {
                    url: tarball_url.to_string(),
                });
            }
        } else if let Some(git_url) = package_svs.strip_prefix("git+") {
            package.source(PackageSource::Git {
                url: git_url.to_string(),
            });
        }
        let pkg_pid = chastefile.add_package(package.build()?)?;
        desc_pid.insert(pkg_desc, pkg_pid);
    }

    for (importer_path, importer) in &lockfile.importers {
        let importer_pid = *importer_to_pid.get(importer_path).unwrap();
        for (dep_name, d) in &importer.dependencies {
            if d.version.starts_with("link:") {
                // TODO:
                continue;
            }
            let dep_desc = format!("{dep_name}@{}", d.version);
            let dep_pid = *desc_pid
                .get(dep_desc.as_str())
                .or_else(|| desc_pid.get(d.version))
                .ok_or_else(|| Error::DependencyPackageNotFound(dep_desc))?;
            let mut dep = DependencyBuilder::new(DependencyKind::Dependency, importer_pid, dep_pid);
            dep.svs(SourceVersionSpecifier::new(d.specifier.to_string())?);
            chastefile.add_dependency(dep.build());
        }
        for (dep_name, d) in &importer.dev_dependencies {
            if d.version.starts_with("link:") {
                // TODO:
                continue;
            }
            let dep_desc = format!("{dep_name}@{}", d.version);
            let dep_pid = *desc_pid
                .get(dep_desc.as_str())
                .or_else(|| desc_pid.get(d.version))
                .ok_or_else(|| Error::DependencyPackageNotFound(dep_desc))?;
            let mut dep =
                DependencyBuilder::new(DependencyKind::DevDependency, importer_pid, dep_pid);
            dep.svs(SourceVersionSpecifier::new(d.specifier.to_string())?);
            chastefile.add_dependency(dep.build());
        }
    }

    for (pkg_desc, snap) in lockfile.snapshots {
        // TODO: handle peer dependencies properly
        // https://codeberg.org/selfisekai/chaste/issues/46
        let pkg_desc = pkg_desc.split_once("(").map(|(s, _)| s).unwrap_or(pkg_desc);
        let pkg_pid = *desc_pid
            .get(pkg_desc)
            .ok_or_else(|| Error::SnapshotNotFound(pkg_desc.to_string()))?;
        for (dep_name, dep_svs) in snap.dependencies {
            // TODO: handle peer dependencies properly
            // https://codeberg.org/selfisekai/chaste/issues/46
            let dep_svs = dep_svs.split_once("(").map(|(s, _)| s).unwrap_or(dep_svs);
            let desc = format!("{dep_name}@{dep_svs}");
            let dep = DependencyBuilder::new(
                DependencyKind::Dependency,
                pkg_pid,
                *desc_pid
                    .get(desc.as_str())
                    .or_else(|| desc_pid.get(dep_svs))
                    .ok_or_else(|| Error::DependencyPackageNotFound(desc))?,
            );
            chastefile.add_dependency(dep.build());
        }
    }

    Ok(chastefile.build()?)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::LazyLock;

    use chaste_types::{Chastefile, Package, PackageID, PackageSourceType};

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
        assert_eq!(doipjs.integrity().hashes.len(), 0);

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
        assert_eq!(minimatch.integrity().hashes.len(), 0);

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
    fn v9_peer_unsatisfied() -> Result<()> {
        let chastefile = test_workspace("v9_peer_unsatisfied")?;
        assert!(!chastefile.packages().into_iter().any(|p| p
            .name()
            .is_some_and(|n| n == "@bazel/bazelisk"
                || n == "@bazel/concatjs"
                || n == "typescript")));

        Ok(())
    }

    #[test]
    fn v9_scope_registry() -> Result<()> {
        let chastefile = test_workspace("v9_scope_registry")?;
        let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
        let empty_pkg = chastefile.package(empty_pid);
        assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
        assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
        assert_eq!(empty_pkg.integrity().hashes.len(), 1);
        assert_eq!(empty_pkg.source_type(), Some(PackageSourceType::Npm));

        Ok(())
    }

    #[test]
    fn v9_tarball_url() -> Result<()> {
        let chastefile = test_workspace("v9_tarball_url")?;
        let empty_pid = chastefile.root_package_dependencies().first().unwrap().on;
        let empty_pkg = chastefile.package(empty_pid);
        assert_eq!(empty_pkg.name().unwrap(), "@a/empty");
        assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
        assert_eq!(empty_pkg.integrity().hashes.len(), 0);
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
        assert!(
            workspace_member_ids.contains(&balls_pid) && workspace_member_ids.contains(&ligma_pid)
        );
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
}
