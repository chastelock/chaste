// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chaste_types::{
    package_name_part, Chastefile, ChastefileBuilder, Checksums, DependencyBuilder, DependencyKind,
    InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageName, PackageSource,
    SourceVersionSpecifier, PACKAGE_JSON_FILENAME,
};
use nom::bytes::complete::tag;
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

    if lockfile.lockfile_version != "9.0" {
        return Err(Error::UnknownLockfileVersion(
            lockfile.lockfile_version.to_string(),
        ));
    }

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
    for (pkg_desc, pkg) in &lockfile.packages {
        let (_, (package_name, _, package_svd)) =
            (package_name, tag("@"), rest)
                .parse(&pkg_desc)
                .map_err(|_| Error::InvalidPackageDescriptor(pkg_desc.to_string()))?;
        let version = pkg
            .version
            .as_deref()
            .or(Some(package_svd))
            .map(|v| v.to_string());
        let mut package =
            PackageBuilder::new(Some(PackageName::new(package_name.to_string())?), version);
        if let Some(integrity) = pkg.resolution.integrity {
            let inte: Integrity = integrity.parse()?;
            if !inte.hashes.is_empty() {
                package.checksums(Checksums::Tarball(inte));
            }
        }
        if let Some(tarball_url) = &pkg.resolution.tarball {
            // If there is a checksum, it's a custom registry.
            if pkg.resolution.integrity.is_some() {
                package.source(PackageSource::Npm);
            } else {
                package.source(PackageSource::TarballURL {
                    url: tarball_url.to_string(),
                });
            }
        } else if let Some(git_url) = package_svd.strip_prefix("git+") {
            package.source(PackageSource::Git {
                url: git_url.to_string(),
            });
        } else if SourceVersionSpecifier::new(package_svd.to_string()).is_ok_and(|svs| svs.is_npm())
        {
            package.source(PackageSource::Npm);
        }
        let pkg_pid = chastefile.add_package(package.build()?)?;
        desc_pid.insert(
            (package_name, package_svd),
            (pkg_pid, &pkg.peer_dependencies, &pkg.peer_dependencies_meta),
        );
    }

    for (importer_path, importer) in &lockfile.importers {
        let importer_pid = *importer_to_pid.get(importer_path).unwrap();
        for (dependencies, kind) in [
            (&importer.dependencies, DependencyKind::Dependency),
            (&importer.dev_dependencies, DependencyKind::DevDependency),
            (&importer.peer_dependencies, DependencyKind::PeerDependency),
            (
                &importer.optional_dependencies,
                DependencyKind::OptionalDependency,
            ),
        ] {
            for (dep_name, d) in dependencies {
                if d.version.starts_with("link:") {
                    // TODO:
                    continue;
                }
                let mut is_aliased = false;
                let dep_pid = {
                    if let Some((dep_pid, _, _)) = desc_pid.get(&(&dep_name, &d.version)) {
                        *dep_pid
                    } else if let Ok((aliased_dep_svd, aliased_dep_name)) =
                        terminated(package_name, tag("@")).parse(&d.version)
                    {
                        if let Some((dep_pid, _, _)) =
                            desc_pid.get(&(aliased_dep_name, aliased_dep_svd))
                        {
                            is_aliased = true;
                            *dep_pid
                        } else {
                            return Err(Error::DependencyPackageNotFound(d.version.to_string()));
                        }
                    } else {
                        return Err(Error::DependencyPackageNotFound(format!(
                            "{dep_name}@{}",
                            d.version
                        )));
                    }
                };
                let mut dep = DependencyBuilder::new(kind, importer_pid, dep_pid);
                if is_aliased {
                    dep.alias_name(PackageName::new(dep_name.to_string())?);
                }
                dep.svs(SourceVersionSpecifier::new(d.specifier.to_string())?);
                chastefile.add_dependency(dep.build());
            }
        }
    }

    for (pkg_desc, snap) in lockfile.snapshots {
        let Some((pkg_svd, pkg_name)) = terminated(package_name, tag("@")).parse(&pkg_desc).ok()
        else {
            unreachable!();
        };
        // TODO: handle peer dependencies properly
        // https://codeberg.org/selfisekai/chaste/issues/46
        let pkg_svd = pkg_svd.split_once("(").map(|(s, _)| s).unwrap_or(pkg_svd);
        let (pkg_pid, _pkg_peers, _pkg_peers_meta) = *desc_pid
            .get(&(pkg_name, pkg_svd))
            .ok_or_else(|| Error::SnapshotNotFound(pkg_desc.to_string()))?;
        for (dependencies, kind) in [
            (&snap.dependencies, DependencyKind::Dependency),
            (&snap.dev_dependencies, DependencyKind::DevDependency),
            (
                &snap.optional_dependencies,
                DependencyKind::OptionalDependency,
            ),
        ] {
            for (dep_name, dep_svd) in dependencies {
                let dep_svd = dep_svd.split_once("(").map(|(s, _)| s).unwrap_or(&dep_svd);
                let dep = DependencyBuilder::new(kind, pkg_pid, {
                    if let Some((dep_pid, _, _)) = desc_pid.get(&(&dep_name, dep_svd)) {
                        *dep_pid
                    } else if let Ok((aliased_dep_svd, aliased_dep_name)) =
                        terminated(package_name, tag("@")).parse(dep_svd)
                    {
                        if let Some((dep_pid, _, _)) =
                            desc_pid.get(&(aliased_dep_name, aliased_dep_svd))
                        {
                            *dep_pid
                        } else {
                            return Err(Error::DependencyPackageNotFound(dep_svd.to_string()));
                        }
                    } else {
                        return Err(Error::DependencyPackageNotFound(format!(
                            "{dep_name}@{dep_svd}"
                        )));
                    }
                });
                chastefile.add_dependency(dep.build());
            }
        }
    }

    Ok(chastefile.build()?)
}
