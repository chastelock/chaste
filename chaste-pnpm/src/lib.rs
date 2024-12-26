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
    dbg!(&lockfile);
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
        package.integrity(pkg.resolution.integrity.parse()?);
        let pkg_pid = chastefile.add_package(package.build()?)?;
        desc_pid.insert(pkg_desc, pkg_pid);
    }

    for (dep_name, d) in &root_importer.dependencies {
        let dep_desc = format!("{dep_name}@{}", d.version);
        let dep_pid = *desc_pid
            .get(dep_desc.as_str())
            .ok_or_else(|| Error::DependencyPackageNotFound(dep_desc))?;
        let mut dep = DependencyBuilder::new(DependencyKind::Dependency, root_pid, dep_pid);
        dep.svd(SourceVersionDescriptor::new(d.specifier.to_string())?);
        chastefile.add_dependency(dep.build());
    }
    for (dep_name, d) in &root_importer.dev_dependencies {
        let dep_desc = format!("{dep_name}@{}", d.version);
        let dep_pid = *desc_pid
            .get(dep_desc.as_str())
            .ok_or_else(|| Error::DependencyPackageNotFound(dep_desc))?;
        let mut dep = DependencyBuilder::new(DependencyKind::DevDependency, root_pid, dep_pid);
        dep.svd(SourceVersionDescriptor::new(d.specifier.to_string())?);
        chastefile.add_dependency(dep.build());
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

    use chaste_types::Chastefile;

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
}
