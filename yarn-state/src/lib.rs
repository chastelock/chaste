// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use nom::branch::alt;
use nom::bytes::complete::{tag, take_until};
use nom::character::complete::{digit1, space1};
use nom::combinator::map_res;
use nom::multi::{many0, many1};
use nom::sequence::{preceded, terminated, tuple};
use nom::IResult;

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

pub fn parse<'a>(input: &'a str) -> Result<YarnState<'a>> {
    statefile(input)
}

fn statefile(input: &str) -> Result<YarnState> {
    match tuple((header, many1(package)))(input) {
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
        tuple((
            many0(tuple((tag("#"), take_until("\n"), tag("\n")))),
            many0(newline),
            tag("__metadata:"),
            newline,
            space1,
            tag("version: "),
        )),
        terminated(map_res(digit1, |n: &str| n.parse()), newline),
    )(input)
}

fn package(input: &str) -> IResult<&str, Package> {
    tuple((
        preceded(
            tuple((newline, tag("\""))),
            terminated(take_until("\":"), tuple((tag("\":"), newline))),
        ),
        many1(package_field),
    ))(input)
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
    alt((package_field_locations, package_field_unknown))(input)
}

fn package_field_locations(input: &str) -> IResult<&str, PackageField> {
    preceded(
        tuple((space1, tag("locations:"), newline)),
        many1(preceded(
            tuple((space1, tag("- \""))),
            terminated(take_until("\""), tuple((tag("\""), newline))),
        )),
    )(input)
    .map(|(input, locations)| (input, PackageField::Locations(locations)))
}

fn package_field_unknown(input: &str) -> IResult<&str, PackageField> {
    let (input, indent) = space1(input)?;
    let (input, _) = tuple((take_until("\n"), tag("\n")))(input)?;
    let (input, _) = many0(tuple((tag(indent), space1, take_until("\n"), tag("\n"))))(input)?;
    Ok((input, PackageField::Unknown))
}

fn newline(input: &str) -> IResult<&str, &str> {
    alt((tag("\n"), tag("\r\n")))(input)
}
