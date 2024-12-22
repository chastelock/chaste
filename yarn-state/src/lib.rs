// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until};
use nom::character::complete::{digit1, space1};
use nom::combinator::map_res;
use nom::multi::{many0, many1};
use nom::sequence::{preceded, terminated};
use nom::{IResult, Parser};

use crate::error::{Error, Result};

pub mod error;

#[non_exhaustive]
pub struct YarnState<'a> {
    pub version: u32,
    pub packages: Vec<Package<'a>>,
}

#[non_exhaustive]
pub struct Package<'a> {
    pub resolution: &'a str,
    pub locations: Vec<&'a str>,
}

pub fn parse(input: &str) -> Result<YarnState<'_>> {
    statefile(input)
}

fn statefile(input: &str) -> Result<YarnState> {
    match (header, many1(package)).parse(input) {
        Ok((input, _)) if !input.is_empty() => {
            dbg!(input);
            Err(Error::InvalidSyntax())
        }
        Err(_) => Err(Error::InvalidSyntax()),

        Ok((_, (version, packages))) => Ok(YarnState { version, packages }),
    }
}

// Returns version number
fn header(input: &str) -> IResult<&str, u32> {
    preceded(
        (
            many0((tag("#"), take_until("\n"), tag("\n"))),
            many0(newline),
            tag("__metadata:"),
            newline,
            space1,
            tag("version: "),
        ),
        terminated(
            map_res(digit1, |n: &str| n.parse()),
            (newline, many0((space1, take_until("\n"), tag("\n")))),
        ),
    )
    .parse(input)
}

fn package(input: &str) -> IResult<&str, Package> {
    (
        preceded(
            (newline, tag("\"")),
            terminated(take_until("\":"), (tag("\":"), newline)),
        ),
        many1(package_field),
    )
        .parse(input)
        .map(|(input, (resolution, fields))| {
            let mut locations = Vec::new();
            for field in fields {
                match field {
                    PackageField::Locations(l) => locations = l,
                    PackageField::Unknown => {}
                }
            }
            (
                input,
                Package {
                    resolution,
                    locations,
                },
            )
        })
}

enum PackageField<'a> {
    Locations(Vec<&'a str>),
    Unknown,
}

fn package_field(input: &str) -> IResult<&str, PackageField> {
    alt((package_field_locations, package_field_unknown)).parse(input)
}

fn package_field_locations(input: &str) -> IResult<&str, PackageField> {
    preceded(
        (space1, tag("locations:"), newline),
        many1(preceded(
            (space1, tag("- \"")),
            terminated(take_until("\""), (tag("\""), newline)),
        )),
    )
    .parse(input)
    .map(|(input, locations)| (input, PackageField::Locations(locations)))
}

fn package_field_unknown(input: &str) -> IResult<&str, PackageField> {
    let (input, indent) = space1(input)?;
    let (input, _) = (take_until("\n"), tag("\n")).parse(input)?;
    let (input, _) = many0((tag(indent), space1, take_until("\n"), tag("\n"))).parse(input)?;
    Ok((input, PackageField::Unknown))
}

fn newline(input: &str) -> IResult<&str, &str> {
    alt((tag("\n"), tag("\r\n"))).parse(input)
}
