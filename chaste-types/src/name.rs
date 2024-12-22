// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::{cmp, fmt};

use nom::bytes::complete::{tag, take_while1};
use nom::combinator::{eof, opt, verify};
use nom::sequence::{preceded, terminated};
use nom::{IResult, Parser};

use crate::error::Result;
use crate::misc::partial_eq_field;
use crate::Error;

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct PackageNamePositions {
    scope_end: Option<usize>,
    pub(crate) total_length: usize,
}

fn package_name_part(input: &str) -> IResult<&str, &str> {
    verify(
        take_while1(|c: char| {
            c.is_ascii_alphanumeric() || c.is_ascii_digit() || ['.', '-', '_'].contains(&c)
        }),
        |part: &str| !part.starts_with("."),
    )
    .parse(input)
}

pub(crate) fn package_name(
    input: &str,
) -> IResult<&str, PackageNamePositions, nom::error::Error<&str>> {
    (
        opt(preceded(tag("@"), terminated(package_name_part, tag("/")))),
        verify(package_name_part, |part: &str| {
            part != "node_modules" && part != "favicon.ico"
        }),
    )
        .parse(input)
        .map(|(inp, (scope, rest))| {
            let scope_end = scope.map(|s| s.len() + 1);
            (
                inp,
                PackageNamePositions {
                    scope_end,
                    total_length: scope_end.map(|e| e + 1).unwrap_or(0) + rest.len(),
                },
            )
        })
}

impl PackageNamePositions {
    fn parse(input: &str) -> Result<Self> {
        terminated(package_name, eof)
            .parse(input)
            .map(|(_, pos)| pos)
            .map_err(|_| crate::Error::InvalidPackageName(input.to_string()))
    }

    /// "@scope" in "@scope/name"
    fn scope(&self) -> Option<(usize, usize)> {
        self.scope_end.map(|end| (0, end))
    }
    /// "@scope/" in "@scope/name"
    fn scope_prefix(&self) -> Option<(usize, usize)> {
        self.scope_end.map(|end| (0, end + 1))
    }
    /// "scope" in "@scope/name"
    fn scope_name(&self) -> Option<(usize, usize)> {
        self.scope_end.map(|end| (1, end))
    }
    /// "name" in "@scope/name"
    fn name_rest(&self) -> (usize, usize) {
        match self.scope_end {
            Some(scope_end) => (scope_end + 1, self.total_length),
            None => (0, self.total_length),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PackageName {
    inner: String,
    positions: PackageNamePositions,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PackageNameBorrowed<'a> {
    pub(crate) inner: &'a str,
    pub(crate) positions: &'a PackageNamePositions,
}

partial_eq_field!(PackageName, inner, String);
partial_eq_field!(PackageName, inner, &str);
partial_eq_field!(PackageNameBorrowed<'_>, inner, String);

impl PartialEq<&str> for PackageNameBorrowed<'_> {
    fn eq(&self, other: &&str) -> bool {
        self.inner.eq(*other)
    }
}

impl PackageNameBorrowed<'_> {
    pub fn to_owned(&self) -> PackageName {
        PackageName {
            inner: self.inner.to_string(),
            positions: self.positions.clone(),
        }
    }
}

macro_rules! option_segment {
    ($name:ident) => {
        pub fn $name(&self) -> Option<&str> {
            self.positions
                .$name()
                .map(|(start, end)| &self.inner[start..end])
        }
    };
}

macro_rules! required_segment {
    ($name:ident) => {
        pub fn $name(&self) -> &str {
            let (start, end) = self.positions.$name();
            &self.inner[start..end]
        }
    };
}

impl PackageName {
    pub fn new(name: String) -> Result<Self> {
        Ok(Self {
            positions: PackageNamePositions::parse(&name)?,
            inner: name,
        })
    }

    pub fn as_borrowed(&self) -> PackageNameBorrowed<'_> {
        PackageNameBorrowed {
            inner: &self.inner,
            positions: &self.positions,
        }
    }

    option_segment!(scope);
    option_segment!(scope_prefix);
    option_segment!(scope_name);
    required_segment!(name_rest);
}

impl<'a> PackageNameBorrowed<'a> {
    option_segment!(scope);
    option_segment!(scope_prefix);
    option_segment!(scope_name);
    required_segment!(name_rest);
}

impl TryFrom<String> for PackageName {
    type Error = Error;

    fn try_from(value: String) -> std::result::Result<Self, Self::Error> {
        PackageName::new(value)
    }
}

impl From<PackageName> for String {
    fn from(value: PackageName) -> Self {
        value.inner
    }
}
impl fmt::Display for PackageName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
impl fmt::Display for PackageNameBorrowed<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}
impl AsRef<str> for PackageNameBorrowed<'_> {
    fn as_ref(&self) -> &str {
        self.inner
    }
}
impl PartialOrd for PackageName {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.inner.cmp(&other.inner))
    }
}
impl PartialOrd for PackageNameBorrowed<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.inner.cmp(other.inner))
    }
}
impl Ord for PackageName {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}
impl Ord for PackageNameBorrowed<'_> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.inner.cmp(other.inner)
    }
}

#[cfg(test)]
mod tests {
    use crate::error::Error;

    use super::PackageName;

    #[test]
    fn test_positions_scoped() {
        let name = PackageName::new("@scope/name".to_string()).unwrap();
        assert_eq!(name.scope(), Some("@scope"));
        assert_eq!(name.scope_name(), Some("scope"));
        assert_eq!(name.scope_prefix(), Some("@scope/"));
        assert_eq!(name.name_rest(), "name");
    }

    #[test]
    fn test_positions_unscoped() {
        let name = PackageName::new("name__1".to_string()).unwrap();
        assert_eq!(name.scope(), None);
        assert_eq!(name.scope_name(), None);
        assert_eq!(name.scope_prefix(), None);
        assert_eq!(name.name_rest(), "name__1");
    }

    #[test]
    fn test_invalid_names() {
        macro_rules! assert_name_error {
            ($name:expr) => {
                assert_eq!(
                    PackageName::new($name.to_string()),
                    Err(Error::InvalidPackageName($name.to_string()))
                );
            };
        }
        assert_name_error!("");
        assert_name_error!("ą");
        assert_name_error!(".bin");
        assert_name_error!("a/");
        assert_name_error!("a@a/a");
        assert_name_error!("@");
        assert_name_error!("@a");
        assert_name_error!("@a/");
        assert_name_error!("/");
        assert_name_error!("@/a");
        assert_name_error!("@/");
        assert_name_error!("@chastelock/node_modules");
        assert_name_error!("node_modules");
    }
}
