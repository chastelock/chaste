// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chaste_types::{
    Chastefile, ChastefileBuilder, DependencyBuilder, DependencyKind, PackageBuilder, PackageName,
    SourceVersionDescriptor, PACKAGE_JSON_FILENAME,
};
use nom::bytes::complete::{tag, take_while1};
use nom::combinator::{opt, recognize, verify};
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
    let package_json_contents = fs::read_to_string(root_dir.join(PACKAGE_JSON_FILENAME))?;
    let package_json: types::PackageJson = serde_json::from_str(&package_json_contents)?;

    let mut chastefile = ChastefileBuilder::new();

    let Some(root_importer) = lockfile.importers.get(".") else {
        return Err(Error::MissingRootImporter);
    };
    let root_pkg = PackageBuilder::new(
        package_json
            .name
            .map(|n| PackageName::new(n.to_string()))
            .transpose()?,
        package_json.version.map(|v| v.to_string()),
    );
    let root_pid = chastefile.add_package(root_pkg.build()?)?;
    chastefile.set_root_package_id(root_pid)?;

    let mut desc_pid = HashMap::with_capacity(lockfile.packages.len());
    for (pkg_desc, pkg) in lockfile.packages {
        let (_, package_name) = package_name(pkg_desc)
            .map_err(|_| Error::InvalidPackageDescriptor(pkg_desc.to_string()))?;
        let mut package =
            PackageBuilder::new(Some(PackageName::new(package_name.to_string())?), None);
        if let Some(integrity) = pkg.resolution.integrity {
            package.integrity(integrity.parse()?);
        }
        let pkg_pid = chastefile.add_package(package.build()?)?;
        desc_pid.insert(pkg_desc, pkg_pid);
    }

    for (importer_path, importer) in &lockfile.importers {
        for (dep_name, d) in &importer.dependencies {
            let dep_desc = format!("{dep_name}@{}", d.version);
            let dep_pid = *desc_pid
                .get(dep_desc.as_str())
                .or_else(|| desc_pid.get(d.version))
                .ok_or_else(|| Error::DependencyPackageNotFound(dep_desc))?;
            let mut dep = DependencyBuilder::new(DependencyKind::Dependency, root_pid, dep_pid);
            dep.svd(SourceVersionDescriptor::new(d.specifier.to_string())?);
            chastefile.add_dependency(dep.build());
        }
        for (dep_name, d) in &importer.dev_dependencies {
            let dep_desc = format!("{dep_name}@{}", d.version);
            let dep_pid = *desc_pid
                .get(dep_desc.as_str())
                .or_else(|| desc_pid.get(d.version))
                .ok_or_else(|| Error::DependencyPackageNotFound(dep_desc))?;
            let mut dep = DependencyBuilder::new(DependencyKind::DevDependency, root_pid, dep_pid);
            dep.svd(SourceVersionDescriptor::new(d.specifier.to_string())?);
            chastefile.add_dependency(dep.build());
        }
    }

    for (pkg_desc, snap) in lockfile.snapshots {
        let pkg_pid = *desc_pid
            .get(pkg_desc)
            .ok_or_else(|| Error::SnapshotNotFound(pkg_desc.to_string()))?;
        for (dep_name, dep_svd) in snap.dependencies {
            let desc = format!("{dep_name}@{dep_svd}");
            let dep = DependencyBuilder::new(
                DependencyKind::Dependency,
                pkg_pid,
                *desc_pid
                    .get(desc.as_str())
                    .or_else(|| desc_pid.get(dep_svd))
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

    use chaste_types::{Chastefile, Package, PackageID};

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
        let semver_dep = root_deps.first().unwrap();
        let svd = semver_dep.svd().unwrap();
        assert!(svd.is_git());
        assert_eq!(svd.ssh_path_sep(), Some(":"));
        let semver = chastefile.package(semver_dep.on);
        assert_eq!(semver.name().unwrap(), "semver");
        // TODO: https://codeberg.org/selfisekai/chaste/issues/45
        // assert_eq!(semver.source_type(), Some(PackageSourceType::Git));

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
        let minimatch_dep = root_dev_deps.first().unwrap();
        let minimatch = chastefile.package(minimatch_dep.on);
        assert_eq!(minimatch.name().unwrap(), "minimatch");
        // TODO: https://codeberg.org/selfisekai/chaste/issues/45
        // assert_eq!(minimatch.source_type(), Some(PackageSourceType::Git));
        assert_eq!(minimatch.integrity().hashes.len(), 0);

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
        // TODO: https://codeberg.org/selfisekai/chaste/issues/45
        // assert_eq!(minimatch.source_type(), Some(PackageSourceType::Git));
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
        // TODO: https://codeberg.org/selfisekai/chaste/issues/43
        // assert_eq!(chalk2.version().unwrap().to_string(), "2.4.2");
        // assert_eq!(chalk5.version().unwrap().to_string(), "5.4.0");

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
        assert!(nop_dep.svd().unwrap().is_npm_tag());

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
        // TODO: https://codeberg.org/selfisekai/chaste/issues/43
        // assert_eq!(empty_pkg.version().unwrap().to_string(), "0.0.1");
        assert_eq!(empty_pkg.integrity().hashes.len(), 1);
        // TODO: recognize custom npm registry.
        assert_eq!(empty_pkg.source_type(), None);

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
}
