// SPDX-FileCopyrightText: 2026 The Chaste Authors
// SPDX-License-Identifier: BSD-2-Clause

use chaste_types::package_name_str;

use nom::bytes::complete::{is_not, tag};
use nom::combinator::{eof, rest};
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

pub fn resolved<'a>(input: &'a str) -> Result<(&'a str, &'a str)> {
    (package_name_str, preceded(tag("@"), rest))
        .parse(input)
        .map(|(_, r)| r)
        .map_err(|_| Error::InvalidResolved(input.to_owned()))
}
