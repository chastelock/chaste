// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};

use chaste_types::{
    package_name_str, ssri, Chastefile, ChastefileBuilder, Checksums, DependencyBuilder,
    DependencyKind, InstallationBuilder, Integrity, ModulePath, PackageBuilder, PackageDerivation,
    PackageDerivationMetaBuilder, PackageID, PackageName, PackagePatchBuilder, PackageSource,
    PackageVersion, SourceVersionSpecifier, PACKAGE_JSON_FILENAME, ROOT_MODULE_PATH,
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

fn npm(input: &str) -> IResult<&str, PackageSource> {
    map(
        preceded(tag("npm:"), map_res(rest, PackageVersion::parse)),
        |_version| PackageSource::Npm,
    )
    .parse(input)
}

fn is_commit_hash(input: &str) -> bool {
    input.len() == 40
        && input
            .as_bytes()
            .iter()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn git_commit(input: &str) -> IResult<&str, PackageSource> {
    map(
        (
            recognize((
                alt((tag("ssh://"), tag("http://"), tag("https://"))),
                take_until::<&str, &str, nom::error::Error<&str>>("#commit="),
            )),
            tag("#commit="),
            verify(rest, is_commit_hash),
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
            |u: &str| {
                !u.contains("?")
                    && !u.contains("#")
                    && (u.ends_with(".tgz")
                        || u.ends_with(".tar.gz")
                        // This landed in yarn 4:
                        || u.rsplit_once("/")
                            .is_some_and(|(_, r)| r.is_empty() && !r.contains(".")))
            },
        ),
        |url| PackageSource::TarballURL {
            url: url.to_string(),
        },
    )
    .parse(input)
}

fn parse_source<'a>(entry: &'a yarn::Entry) -> Option<(&'a str, Option<PackageSource>)> {
    match (
        terminated(package_name_str, tag("@")),
        opt(alt((npm, git_commit, tarball_url))),
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
    if !entry.integrity.is_empty() {
        pkg.checksums(parse_checksum(entry.integrity)?);
    }
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

type ResolutionParent<'a> = (&'a str, Option<&'a str>);
type Resolution<'a> = (Option<ResolutionParent<'a>>, &'a str);

fn parse_resolution_key<'a>(input: &'a str) -> Result<Resolution<'a>> {
    (
        opt(terminated(
            (
                package_name_str,
                opt(preceded(tag("@"), until_just_package_name_is_left)),
            ),
            tag("/"),
        )),
        terminated(package_name_str, eof),
    )
        .parse(input)
        .map(|(_, r)| r)
        .map_err(|_| Error::InvalidResolution(input.to_string()))
}

fn resolution_from_state_key(state_key: &str) -> Cow<'_, str> {
    if state_key.len() > 137 {
        // "tsec@virtual:ea43cfe65230d5ab1f93db69b01a1f672ecef3abbfb61f3ac71a2f930c090b853c9c93d03a1e3590a6d9dfed177d3a468279e756df1df2b5720d71b64487719c#npm:0.2.8"
        if let Ok((_, (package_name, _virt, _hex, _hash_char, descriptor))) = (
            package_name_str,
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
    root_package_json: &PackageJson,
    resolutions: &HashMap<Resolution, &str>,
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
        e.descriptors.iter().any(|(d_n, d_s_raw)| {
            // The SVS can have additional parameters added.
            // "name@patch:name@0.1.0#./file.patch::locator=%40chastelock%2Ftestcase%40workspace%3A."
            let d_s = d_s_raw.rsplit_once("::").map(|(l, _)| l).unwrap_or(d_s_raw);
            *d_n == descriptor_name
                && (d_s == descriptor_svs
                    || d_s.strip_prefix("npm:") == Some(descriptor_svs)
                    || candidate_resolutions
                        .iter()
                        .any(|r_s| d_s == **r_s || d_s.strip_prefix("npm:") == Some(r_s)))
        })
    });
    if let Some((idx, _)) = candidate_entries.next() {
        if candidate_entries.next().is_some() {
            return Err(Error::AmbiguousResolution(format!(
                "{descriptor_name}@{descriptor_svs}",
            )));
        }
        return Ok(Some(*index_to_pid.get(&idx).unwrap()));
    }

    if is_peer {
        // Peer dependencies are cursed. If multiple modules *peer* depend on the same module name,
        // Yarn will resolve them to one module, even if it considers them conflicting.
        // I mean, it can't nest them. Retry, only matching the name and not the SVS.
        let candidate_entries = yarn_lock
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                e.descriptors
                    .iter()
                    .any(|(d_n, _d_s)| *d_n == descriptor_name)
            })
            .collect::<Vec<_>>();

        // If there's just one candidate to consider, it's easy.
        if candidate_entries.len() == 1 {
            return Ok(Some(*index_to_pid.get(&candidate_entries[0].0).unwrap()));
        }
        // Peer dependencies can be optional.
        // TODO: also check peerDependenciesMeta
        if candidate_entries.len() == 0 {
            return Ok(None);
        }

        // If the root module also depends on it directly, it has to be directly in node_modules/,
        // where a peer dependency would be.
        for root_deps in [
            &root_package_json.dependencies,
            &root_package_json.dev_dependencies,
            &root_package_json.optional_dependencies,
            &root_package_json.peer_dependencies,
        ] {
            if let Some(alt_svs) = root_deps.get(descriptor_name) {
                if let Some((alt_candidate_index, _)) = candidate_entries.iter().find(|(_, e)| {
                    e.descriptors.iter().any(|ed| {
                        ed.0 == descriptor_name
                            && (ed.1 == alt_svs || ed.1.strip_prefix("npm:") == Some(alt_svs))
                    })
                }) {
                    return Ok(Some(*index_to_pid.get(alt_candidate_index).unwrap()));
                }
            }
        }

        // TODO: Check yarn-state if available?

        return Err(Error::AmbiguousResolution(format!(
            "{descriptor_name}@{descriptor_svs}",
        )));
    }
    Err(Error::DependencyNotFound(format!(
        "{descriptor_name}@{descriptor_svs}",
    )))
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

    let mut deferred_pkgs = Vec::new();

    for (index, entry) in yarn_lock.entries.iter().enumerate() {
        let pkg = parse_package(entry)?;

        // For patch: packages, we need to mark the derivation, for which
        // we need the PackageID they're derived from.
        if (package_name_str, tag("@patch:"))
            .parse(entry.resolved)
            .is_ok()
        {
            deferred_pkgs.push((index, entry, pkg));
            continue;
        }

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
    for (index, entry, mut pkg) in deferred_pkgs {
        let Ok((
            _,
            (_, _, patched_pkg_name, _, patched_pkg_svd_penc, _, patch_path, _, _patch_meta),
        )) = (
            package_name_str,
            tag("@patch:"),
            package_name_str,
            tag("@"),
            take_until("#"),
            tag("#"),
            take_until("::"),
            tag("::"),
            rest,
        )
            .parse(entry.resolved)
        else {
            return Err(Error::InvalidResolved(entry.resolved.to_string()));
        };
        // "npm%3A0.1.0" -> "npm:0.1.0"
        let patched_pkg_svd =
            percent_encoding::percent_decode_str(patched_pkg_svd_penc).decode_utf8()?;
        let mut source_candidates = yarn_lock.entries.iter().enumerate().filter(|(_, e)| {
            (
                tag::<&str, &str, ()>(patched_pkg_name),
                tag("@"),
                tag(patched_pkg_svd.as_ref()),
                eof,
            )
                .parse(e.resolved)
                .is_ok()
        });
        let Some((source_idx, _)) = source_candidates.next() else {
            return Err(Error::InvalidResolved(entry.resolved.to_string()));
        };
        if source_candidates.next().is_some() {
            return Err(Error::AmbiguousResolved(entry.resolved.to_string()));
        }
        let source_pid = *index_to_pid.get(&source_idx).unwrap();
        let patch = PackagePatchBuilder::new(
            patch_path
                .strip_prefix("./")
                .unwrap_or(patch_path)
                .to_string(),
        );
        pkg.derived(
            PackageDerivationMetaBuilder::new(PackageDerivation::Patch(patch.build()?), source_pid)
                .build()?,
        );
        let pid = chastefile_builder.add_package(pkg.build()?)?;
        index_to_pid.insert(index, pid);
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

        // In berry, dependencies are stored in 2 sections under an Entry:
        // either "peerDependencies:", "dependencies:".
        // If they are optional, this will be indicated in "peerDependenciesMeta:",
        // same as in a package.json, or, "dependenciesMeta:", presumably an analogy to that.

        // In classic, they were stored in an "optionalDependencies:" section,
        // but, as per above, this shouldn't be a thing here.
        debug_assert!(entry.optional_dependencies.is_empty());

        for (dependencies, deps_metas, kind_) in [
            (
                &entry.dependencies,
                &entry.dependencies_meta,
                DependencyKind::Dependency,
            ),
            (
                &entry.peer_dependencies,
                &entry.peer_dependencies_meta,
                DependencyKind::PeerDependency,
            ),
        ] {
            for dep_descriptor in dependencies {
                let deps_meta = deps_metas
                    .iter()
                    .find(|(k, _)| *k == dep_descriptor.0)
                    .map(|(_, v)| v);
                let kind = match (deps_meta.map(|m| m.optional), kind_) {
                    (Some(Some(true)), DependencyKind::PeerDependency) => {
                        DependencyKind::OptionalPeerDependency
                    }
                    (Some(Some(true)), DependencyKind::Dependency) => {
                        DependencyKind::OptionalDependency
                    }
                    (Some(Some(true)), _) => unreachable!(),
                    (_, k) => k,
                };
                let Some(dep_pid) = find_dep_pid(
                    dep_descriptor,
                    &yarn_lock,
                    &root_package_json,
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
