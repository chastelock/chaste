// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;

use chaste_types::{package_name_str, PackageSource, PackageVersion};

use nom::branch::alt;
use nom::bytes::complete::{tag, take, take_until};
use nom::combinator::{map, map_res, opt, recognize, rest, verify};
use nom::sequence::{preceded, terminated};
use nom::{IResult, Parser};
use yarn_lock_parser as yarn;

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

pub(super) fn parse_source<'a>(entry: &'a yarn::Entry) -> Option<(&'a str, Option<PackageSource>)> {
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

pub(super) fn resolution_from_state_key(state_key: &str) -> Cow<'_, str> {
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

pub(super) fn patch_descriptor(input: &str) -> Option<(&str, &str, &str, &str, &str)> {
    (
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
        .parse(input)
        .ok()
        .map(|(_, v)| (v.0, v.2, v.4, v.6, v.8))
}
