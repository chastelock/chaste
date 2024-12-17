// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;
use std::path::Path;

use chaste_types::{
    ssri, Chastefile, ChastefileBuilder, DependencyBuilder, DependencyKind, InstallationBuilder,
    Integrity, PackageBuilder, PackageID, PackageName, PackageSource, PackageVersion,
    SourceVersionDescriptor,
};
use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    combinator::{map, map_res, opt, recognize, rest, verify},
    sequence::{preceded, terminated, tuple},
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
    )(input)
}

fn package_name(input: &str) -> IResult<&str, &str, nom::error::Error<&str>> {
    recognize(tuple((
        opt(preceded(tag("@"), terminated(package_name_part, tag("/")))),
        verify(package_name_part, |part: &str| {
            part != "node_modules" && part != "favicon.ico"
        }),
    )))(input)
}

fn npm(input: &str) -> IResult<&str, PackageSource> {
    map(
        preceded(tag("npm:"), map_res(rest, PackageVersion::parse)),
        |_version| PackageSource::Npm,
    )(input)
}

fn ssh(input: &str) -> IResult<&str, PackageSource> {
    map(
        tuple((
            recognize(tuple((
                tag::<&str, &str, nom::error::Error<&str>>("ssh://"),
                take_until::<&str, &str, nom::error::Error<&str>>("#commit="),
            ))),
            tag("#commit="),
            rest,
        )),
        |(url, _, _)| PackageSource::Git {
            url: url.to_string(),
        },
    )(input)
}

fn parse_source<'a>(entry: &'a yarn::Entry) -> Option<PackageSource> {
    match preceded(terminated(package_name, tag("@")), opt(alt((npm, ssh))))(entry.resolved) {
        Ok((remaining_input, output)) if remaining_input.is_empty() => output,
        Ok((_, _)) => None,
        Err(_e) => None,
    }
}

fn parse_package(entry: &yarn::Entry) -> Result<PackageBuilder> {
    let mut pkg = PackageBuilder::new(
        Some(PackageName::new(entry.name.to_string())?),
        Some(entry.version.to_string()),
    );
    let integrity: Integrity = Integrity::from_hex(entry.integrity, ssri::Algorithm::Sha512)?;
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
    let (descriptor_name, descriptor_svd) = (descriptor.0.as_ref(), descriptor.1.as_ref());
    if let Some((idx, _)) = yarn_lock.entries.iter().enumerate().find(|(_, e)| {
        e.descriptors.iter().any(|(d_n, d_s)| {
            *d_n == descriptor_name
                && (*d_s == descriptor_svd || d_s.strip_prefix("npm:") == Some(descriptor_svd))
        })
    }) {
        Ok(*index_to_pid.get(&idx).unwrap())
    } else {
        Err(Error::DependencyNotFound(format!(
            "{0}@{1}",
            descriptor_name, descriptor_svd
        )))
    }
}

pub(crate) fn resolve<'a>(yarn_lock: yarn::Lockfile<'a>, root_dir: &Path) -> Result<Chastefile> {
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
            .find_map(|(_, e_svd)| e_svd.strip_prefix("workspace:"))
        {
            if workspace_path == "." {
                chastefile_builder.set_root_package_id(pid)?;
                chastefile_builder.add_package_installation(
                    InstallationBuilder::new(pid, "".to_string()).build()?,
                );
            } else {
                chastefile_builder.set_as_workspace_member(pid)?;
                chastefile_builder.add_package_installation(
                    InstallationBuilder::new(pid, workspace_path.to_string()).build()?,
                );
            }
        }
    }

    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let from_pid = index_to_pid.get(&index).unwrap();
        for dep_descriptor in &entry.dependencies {
            let dep_pid = find_dep_pid(&dep_descriptor, &yarn_lock, &index_to_pid)?;
            let mut dep = DependencyBuilder::new(DependencyKind::Dependency, *from_pid, dep_pid);
            dep.svd(SourceVersionDescriptor::new(dep_descriptor.1.to_string())?);
            chastefile_builder.add_dependency(dep.build());
        }
    }
    Ok(chastefile_builder.build()?)
}
