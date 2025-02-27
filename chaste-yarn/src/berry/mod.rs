// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};

use chaste_types::{
    package_name_part, ssri, Chastefile, ChastefileBuilder, Checksums, DependencyBuilder,
    DependencyKind, InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageID,
    PackageName, PackageSource, PackageVersion, SourceVersionSpecifier, PACKAGE_JSON_FILENAME,
    ROOT_MODULE_PATH,
};
use nom::branch::alt;
use nom::bytes::complete::{tag, take, take_until};
use nom::combinator::{eof, map, map_res, opt, recognize, rest, verify};
use nom::sequence::{preceded, terminated};
use nom::{IResult, Parser};
use yarn_lock_parser as yarn;

use crate::berry::types::PackageJson;
use crate::error::{Error, Result};

mod types;

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

/// Note: it must end with ".tgz" in berry.
fn tarball_url(input: &str) -> IResult<&str, PackageSource> {
    map(
        verify(
            recognize((
                alt((
                    tag::<&str, &str, nom::error::Error<&str>>("http://"),
                    tag::<&str, &str, nom::error::Error<&str>>("https://"),
                )),
                rest,
            )),
            |u: &str| !u.contains("?") && !u.contains("#") && u.ends_with(".tgz"),
        ),
        |url| PackageSource::TarballURL {
            url: url.to_string(),
        },
    )
    .parse(input)
}

fn parse_source<'a>(entry: &'a yarn::Entry) -> Option<(&'a str, Option<PackageSource>)> {
    match (
        terminated(package_name, tag("@")),
        opt(alt((npm, ssh, tarball_url))),
    )
        .parse(entry.resolved)
    {
        Ok(("", output)) => Some(output),
        Ok((_, _)) => None,
        Err(_e) => None,
    }
}

fn parse_checksum(integrity: &str) -> Result<Checksums> {
    // In v8 lockfiles, there is a prefix like "10/".
    let integrity = integrity
        .split_once("/")
        .map(|(_, i)| i)
        .unwrap_or(integrity);
    Ok(Checksums::RepackZip(Integrity::from_hex(
        integrity,
        ssri::Algorithm::Sha512,
    )?))
}

fn parse_package(entry: &yarn::Entry) -> Result<PackageBuilder> {
    let source = parse_source(entry);
    let name = match &source {
        Some((n, _)) => n,
        _ => entry.name,
    };
    let mut pkg = PackageBuilder::new(
        Some(PackageName::new(name.to_string())?),
        Some(entry.version.to_string()),
    );
    pkg.checksums(parse_checksum(entry.integrity)?);
    if let Some((_, Some(source))) = source {
        pkg.source(source);
    }
    Ok(pkg)
}

fn until_just_package_name_is_left(input: &str) -> IResult<&str, &str> {
    let last_slash = input.rfind("/");
    if let Some((il, ir)) = last_slash.map(|lsi| (&input[..lsi], &input[lsi..])) {
        let previous_slash = il.rfind("/");
        if let Some((pl, pr)) = previous_slash.map(|lsi| (&input[..lsi], &input[lsi..])) {
            if !pl.is_empty() && pr.starts_with("/@") {
                return Ok((pr, pl));
            }
        }
        return Ok((ir, il));
    }
    Err(nom::Err::Failure(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Verify,
    )))
}

fn parse_resolution_key(input: &str) -> Result<(Option<(&str, Option<&str>)>, &str)> {
    (
        opt(terminated(
            (
                package_name,
                opt(preceded(tag("@"), until_just_package_name_is_left)),
            ),
            tag("/"),
        )),
        terminated(package_name, eof),
    )
        .parse(input)
        .map(|(_, r)| r)
        .map_err(|_| Error::InvalidResolution(input.to_string()))
}

fn resolution_from_state_key(state_key: &str) -> Cow<'_, str> {
    if state_key.len() > 137 {
        // "tsec@virtual:ea43cfe65230d5ab1f93db69b01a1f672ecef3abbfb61f3ac71a2f930c090b853c9c93d03a1e3590a6d9dfed177d3a468279e756df1df2b5720d71b64487719c#npm:0.2.8"
        if let Ok((_, (package_name, _virt, _hex, _hash_char, descriptor))) = (
            package_name,
            tag("@virtual:"),
            verify(take(128usize), |hex: &str| {
                hex.as_bytes()
                    .iter()
                    .all(|b| (b'a'..=b'f').contains(b) || b.is_ascii_digit())
            }),
            tag("#"),
            rest,
        )
            .parse(state_key)
        {
            return Cow::Owned(format!("{package_name}@{descriptor}"));
        }
    }
    Cow::Borrowed(state_key)
}

fn find_dep_pid<'a, S>(
    descriptor: &'a (S, S),
    yarn_lock: &'a yarn::Lockfile<'a>,
    resolutions: &HashMap<(Option<(&str, Option<&str>)>, &str), &str>,
    index_to_pid: &HashMap<usize, PackageID>,
    is_peer: bool,
) -> Result<Option<PackageID>>
where
    S: AsRef<str>,
{
    let (descriptor_name, descriptor_svs) = (descriptor.0.as_ref(), descriptor.1.as_ref());
    let candidate_resolutions = resolutions
        .iter()
        .filter(|((_, pn), _svs)| *pn == descriptor_name)
        .map(|(_, svs)| svs)
        .collect::<Vec<&&str>>();
    let mut candidate_entries = yarn_lock.entries.iter().enumerate().filter(|(_, e)| {
        e.descriptors.iter().any(|(d_n, d_s)| {
            *d_n == descriptor_name
                && (*d_s == descriptor_svs
                    || d_s.strip_prefix("npm:") == Some(descriptor_svs)
                    || candidate_resolutions
                        .iter()
                        .any(|r_s| d_s == *r_s || d_s.strip_prefix("npm:") == Some(r_s)))
        })
    });
    if let Some((idx, _)) = candidate_entries.next() {
        if candidate_entries.next().is_some() {
            return Err(Error::AmbiguousResolution(format!(
                "{0}@{1}",
                descriptor_name, descriptor_svs
            )));
        }
        Ok(Some(*index_to_pid.get(&idx).unwrap()))
    } else if is_peer {
        // Peer dependencies are cursed. If multiple modules *peer* depend on the same module name,
        // Yarn will resolve them to one module, even if it considers them conflicting.
        // I mean, it can't nest them. Retry, only matching the name and not the SVS.
        let mut candidate_entries = yarn_lock.entries.iter().enumerate().filter(|(_, e)| {
            e.descriptors
                .iter()
                .any(|(d_n, _d_s)| *d_n == descriptor_name)
        });
        if let Some((idx, _)) = candidate_entries.next() {
            if candidate_entries.next().is_some() {
                return Err(Error::AmbiguousResolution(format!(
                    "{0}@{1}",
                    descriptor_name, descriptor_svs
                )));
            }
            Ok(Some(*index_to_pid.get(&idx).unwrap()))
        } else {
            // They can also be optional.
            Ok(None)
        }
    } else {
        Err(Error::DependencyNotFound(format!(
            "{0}@{1}",
            descriptor_name, descriptor_svs
        )))
    }
}

pub(crate) fn resolve(yarn_lock: yarn::Lockfile<'_>, root_dir: &Path) -> Result<Chastefile> {
    let root_package_contents = fs::read_to_string(root_dir.join(PACKAGE_JSON_FILENAME))?;
    let root_package_json: PackageJson = serde_json::from_str(&root_package_contents)?;

    let mut resolutions = HashMap::new();
    for (rk, rv) in &root_package_json.resolutions {
        resolutions.insert(parse_resolution_key(rk)?, rv.as_ref());
    }

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
            let expected_resolution = resolution_from_state_key(st8_pkg.resolution);
            let (p_idx, _) = yarn_lock
                .entries
                .iter()
                .enumerate()
                .find(|(_, e)| e.resolved == expected_resolution)
                .ok_or_else(|| Error::StatePackageNotFound(expected_resolution.to_string()))?;
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
        for (dependencies, kind) in [
            (&entry.dependencies, DependencyKind::Dependency),
            (&entry.peer_dependencies, DependencyKind::PeerDependency),
        ] {
            for dep_descriptor in dependencies {
                let Some(dep_pid) = find_dep_pid(
                    dep_descriptor,
                    &yarn_lock,
                    &resolutions,
                    &index_to_pid,
                    kind.is_peer(),
                )?
                else {
                    continue;
                };
                let mut dep = DependencyBuilder::new(kind, *from_pid, dep_pid);
                let svs = SourceVersionSpecifier::new(dep_descriptor.1.to_string())?;
                if svs.aliased_package_name().is_some() {
                    dep.alias_name(PackageName::new(dep_descriptor.0.to_string())?);
                }
                dep.svs(svs);
                chastefile_builder.add_dependency(dep.build());
            }
        }
    }
    Ok(chastefile_builder.build()?)
}

#[cfg(test)]
mod tests {
    use crate::error::Result;

    use super::parse_resolution_key;

    #[test]
    fn resolution_keys() -> Result<()> {
        assert_eq!(parse_resolution_key("lodash")?, (None, "lodash"));
        assert_eq!(
            parse_resolution_key("@chastelock/testcase")?,
            (None, "@chastelock/testcase")
        );
        assert_eq!(
            parse_resolution_key("lodash/@chastelock/testcase")?,
            (Some(("lodash", None)), "@chastelock/testcase")
        );
        assert_eq!(
            parse_resolution_key("lodash@1/react")?,
            (Some(("lodash", Some("1"))), "react")
        );
        assert_eq!(
            parse_resolution_key("lodash@1/@chastelock/testcase")?,
            (Some(("lodash", Some("1"))), "@chastelock/testcase")
        );

        Ok(())
    }
}
