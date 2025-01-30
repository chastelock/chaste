// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chaste_types::{
    Chastefile, ChastefileBuilder, Checksums, DependencyBuilder, DependencyKind,
    InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageName, PackageSource,
    SourceVersionSpecifier, PACKAGE_JSON_FILENAME,
};
use nom::bytes::complete::{tag, take_while1};
use nom::combinator::{opt, recognize, rest, verify};
use nom::sequence::{preceded, terminated};
use nom::{IResult, Parser};

pub use crate::error::Error;
use crate::error::Result;

mod error;
#[cfg(test)]
mod tests;
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
            let inte: Integrity = integrity.parse()?;
            if !inte.hashes.is_empty() {
                package.checksums(Checksums::Tarball(inte));
            }
        }
        if let Some(tarball_url) = pkg.resolution.tarball {
            // If there is a checksum, it's a custom registry.
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
        } else if SourceVersionSpecifier::new(package_svs.to_string()).is_ok_and(|svs| svs.is_npm())
        {
            package.source(PackageSource::Npm);
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
            let mut is_aliased = false;
            let dep_pid = *desc_pid
                .get(dep_desc.as_str())
                .or_else(|| {
                    is_aliased = true;
                    desc_pid.get(d.version)
                })
                .ok_or(Error::DependencyPackageNotFound(dep_desc))?;
            let mut dep = DependencyBuilder::new(DependencyKind::Dependency, importer_pid, dep_pid);
            if is_aliased {
                dep.alias_name(PackageName::new(dep_name.to_string())?);
            }
            dep.svs(SourceVersionSpecifier::new(d.specifier.to_string())?);
            chastefile.add_dependency(dep.build());
        }
        for (dep_name, d) in &importer.dev_dependencies {
            if d.version.starts_with("link:") {
                // TODO:
                continue;
            }
            let dep_desc = format!("{dep_name}@{}", d.version);
            let mut is_aliased = false;
            let dep_pid = *desc_pid
                .get(dep_desc.as_str())
                .or_else(|| {
                    is_aliased = true;
                    desc_pid.get(d.version)
                })
                .ok_or(Error::DependencyPackageNotFound(dep_desc))?;
            let mut dep =
                DependencyBuilder::new(DependencyKind::DevDependency, importer_pid, dep_pid);
            if is_aliased {
                dep.alias_name(PackageName::new(dep_name.to_string())?);
            }
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
                    .ok_or(Error::DependencyPackageNotFound(desc))?,
            );
            chastefile.add_dependency(dep.build());
        }
    }

    Ok(chastefile.build()?)
}
