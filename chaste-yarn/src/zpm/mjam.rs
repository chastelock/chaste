// SPDX-FileCopyrightText: 2026 The Chaste Authors
// SPDX-License-Identifier: BSD-2-Clause

use chaste_types::{package_name_str, PackageSource};

use nom::branch::alt;
use nom::bytes::complete::{is_not, tag};
use nom::combinator::{eof, map, opt, peek, rest, verify};
use nom::multi::separated_list1;
use nom::sequence::{preceded, terminated};
use nom::{IResult, Parser as _};

use crate::error::{Error, Result};

pub fn specifier<'a>(input: &'a str) -> IResult<&'a str, (&'a str, &'a str)> {
    (package_name_str, preceded(tag("@"), is_not(","))).parse(input)
}

pub fn specifiers<'a>(input: &'a str) -> Result<Vec<(&'a str, &'a str)>> {
    terminated(separated_list1(tag(", "), specifier), eof)
        .parse(input)
        .map(|(_, s)| s)
        .map_err(|_| Error::InvalidEntryKey(input.to_owned()))
}

pub enum Resolved<'a> {
    Remote(PackageSource),
    Workspace(&'a str),
}

pub fn resolved_source<'a>(input: &'a str) -> IResult<&'a str, Resolved<'a>> {
    alt((
        preceded(
            tag("npm:"),
            map(rest, |_| Resolved::Remote(PackageSource::Npm)),
        ),
        preceded(
            tag("git:"),
            map(rest, |i: &str| {
                Resolved::Remote(PackageSource::Git { url: i.to_owned() })
            }),
        ),
        preceded(tag("workspace:"), map(rest, |i| Resolved::Workspace(i))),
        preceded(
            peek(alt((tag("https://"), tag("http://")))),
            map(
                verify(rest, |i: &str| {
                    i.ends_with(".tgz") || i.ends_with(".tar.gz")
                }),
                |i: &str| Resolved::Remote(PackageSource::TarballURL { url: i.to_owned() }),
            ),
        ),
    ))
    .parse(input)
}

pub fn resolved<'a>(input: &'a str) -> Result<(&'a str, Option<Resolved<'a>>)> {
    (package_name_str, preceded(tag("@"), opt(resolved_source)))
        .parse(input)
        .map(|(_, r)| r)
        .map_err(|_| Error::InvalidResolved(input.to_owned()))
}
