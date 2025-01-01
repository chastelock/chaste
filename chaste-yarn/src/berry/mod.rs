// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};

use chaste_types::{
    ssri, Chastefile, ChastefileBuilder, DependencyBuilder, DependencyKind, InstallationBuilder,
    Integrity, ModulePath, PackageBuilder, PackageID, PackageName, PackageSource, PackageVersion,
    SourceVersionSpecifier, ROOT_MODULE_PATH,
};
use nom::Parser;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    combinator::{map, map_res, opt, recognize, rest, verify},
    sequence::{preceded, terminated},
    IResult,
};
use yarn_lock_parser as yarn;

use crate::error::{Error, Result};

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

fn npm(input: &str) -> IResult<&str, PackageSource> {
    map(
        preceded(tag("npm:"), map_res(rest, PackageVersion::parse)),
        |_version| PackageSource::Npm,
    )
    .parse(input)
}

fn ssh(input: &str) -> IResult<&str, PackageSource> {
    map(
        (
            recognize((
                tag::<&str, &str, nom::error::Error<&str>>("ssh://"),
                take_until::<&str, &str, nom::error::Error<&str>>("#commit="),
            )),
            tag("#commit="),
            rest,
        ),
        |(url, _, _)| PackageSource::Git {
            url: url.to_string(),
        },
    )
    .parse(input)
}

fn parse_source(entry: &yarn::Entry) -> Option<PackageSource> {
    match preceded(terminated(package_name, tag("@")), opt(alt((npm, ssh)))).parse(entry.resolved) {
        Ok(("", output)) => output,
        Ok((_, _)) => None,
        Err(_e) => None,
    }
}

fn parse_checksum(integrity: &str) -> Result<Integrity> {
    // In v8 lockfiles, there is a prefix like "10/".
    let integrity = integrity
        .split_once("/")
        .map(|(_, i)| i)
        .unwrap_or(integrity);
    Ok(Integrity::from_hex(integrity, ssri::Algorithm::Sha512)?)
}

fn parse_package(entry: &yarn::Entry) -> Result<PackageBuilder> {
    let mut pkg = PackageBuilder::new(
        Some(PackageName::new(entry.name.to_string())?),
        Some(entry.version.to_string()),
    );
    let integrity: Integrity = parse_checksum(entry.integrity)?;
    if let Some(source) = parse_source(entry) {
        pkg.source(source);
    }
    pkg.integrity(integrity);
    Ok(pkg)
}

fn find_dep_pid<'a, S>(
    descriptor: &'a (S, S),
    yarn_lock: &'a yarn::Lockfile<'a>,
    index_to_pid: &HashMap<usize, PackageID>,
) -> Result<PackageID>
where
    S: AsRef<str>,
{
    let (descriptor_name, descriptor_svs) = (descriptor.0.as_ref(), descriptor.1.as_ref());
    if let Some((idx, _)) = yarn_lock.entries.iter().enumerate().find(|(_, e)| {
        e.descriptors.iter().any(|(d_n, d_s)| {
            *d_n == descriptor_name
                && (*d_s == descriptor_svs || d_s.strip_prefix("npm:") == Some(descriptor_svs))
        })
    }) {
        Ok(*index_to_pid.get(&idx).unwrap())
    } else {
        Err(Error::DependencyNotFound(format!(
            "{0}@{1}",
            descriptor_name, descriptor_svs
        )))
    }
}

pub(crate) fn resolve(yarn_lock: yarn::Lockfile<'_>, root_dir: &Path) -> Result<Chastefile> {
    let mut chastefile_builder = ChastefileBuilder::new();
    let mut index_to_pid: HashMap<usize, PackageID> =
        HashMap::with_capacity(yarn_lock.entries.len());

    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let pkg = parse_package(entry)?;
        let pid = chastefile_builder.add_package(pkg.build()?)?;
        index_to_pid.insert(index, pid);
        if let Some(workspace_path) = entry
            .descriptors
            .iter()
            .find_map(|(_, e_svs)| e_svs.strip_prefix("workspace:"))
        {
            if workspace_path == "." {
                chastefile_builder.set_root_package_id(pid)?;
                let installation_builder = InstallationBuilder::new(pid, ROOT_MODULE_PATH.clone());
                chastefile_builder.add_package_installation(installation_builder.build()?);
            } else {
                chastefile_builder.set_as_workspace_member(pid)?;
                chastefile_builder.add_package_installation(
                    InstallationBuilder::new(pid, ModulePath::new(workspace_path.to_string())?)
                        .build()?,
                );
            }
        }
    }

    let maybe_state_contents =
        match fs::read_to_string(root_dir.join("node_modules").join(".yarn-state.yml")) {
            Ok(s) => Some(s),
            Err(e) if e.kind() == io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
    let maybe_state = maybe_state_contents
        .as_ref()
        .map(|sc| yarn_state::parse(sc))
        .transpose()?;
    if let Some(state) = maybe_state {
        for st8_pkg in &state.packages {
            let (p_idx, _) = yarn_lock
                .entries
                .iter()
                .enumerate()
                .find(|(_, e)| e.resolved == st8_pkg.resolution)
                .ok_or_else(|| Error::StatePackageNotFound(st8_pkg.resolution.to_string()))?;
            let pid = index_to_pid.get(&p_idx).unwrap();
            for st8_location in &st8_pkg.locations {
                let installation =
                    InstallationBuilder::new(*pid, ModulePath::new(st8_location.to_string())?)
                        .build()?;
                chastefile_builder.add_package_installation(installation);
            }
        }
    }

    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let from_pid = index_to_pid.get(&index).unwrap();
        for dep_descriptor in &entry.dependencies {
            let dep_pid = find_dep_pid(dep_descriptor, &yarn_lock, &index_to_pid)?;
            let mut dep = DependencyBuilder::new(DependencyKind::Dependency, *from_pid, dep_pid);
            dep.svs(SourceVersionSpecifier::new(dep_descriptor.1.to_string())?);
            chastefile_builder.add_dependency(dep.build());
        }
    }
    Ok(chastefile_builder.build()?)
}
