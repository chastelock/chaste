// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use nom::branch::alt;
use nom::bytes::complete::{tag, take_while, take_while1};
use nom::combinator::{eof, opt, recognize, rest, verify};
use nom::sequence::{preceded, tuple};

use crate::error::Result;
use crate::name::PackageNamePositions;

/// Source/version descriptor. It is a constraint defined by a specific [`crate::Dependency`]
/// rather than by a [`crate::PackageSource`].
#[derive(Debug)]
pub struct SourceVersionDescriptor {
    inner: String,
    positions: SourceVersionDescriptorPositions,
}

#[derive(Debug)]
enum SourceVersionDescriptorPositions {
    Npm {
        type_prefix_end: usize,
        alias_package_name: Option<PackageNamePositions>,
    },
    TarballURL {},
    Git {
        type_prefix_end: usize,
    },
    GitHub {
        type_prefix_end: usize,
    },
}

fn npm(input: &str) -> Option<SourceVersionDescriptorPositions> {
    tuple((
        opt(tag("npm:")),
        opt(|input| {
            let (input, alias_package_name) = PackageNamePositions::parse_remaining(input, true)
                .map(|(i, p)| (i, Some(p)))
                .unwrap_or((input, None));
            let (input, _) = tag("@")(input)?;
            Ok((input, alias_package_name))
        }),
        |input| {
            // "" is a valid version specifier, but Range does not accept it.
            // Override it to a working equivalent.
            let range = nodejs_semver::Range::parse(if input == "" { "*" } else { input })
                .map_err(|_| nom::Err::Error(()))?;
            Ok(("", range))
        },
        eof,
    ))(input)
    .ok()
    .map(|(_, (type_prefix, alias_package_name, _range, _))| {
        let alias_package_name = alias_package_name.flatten();
        SourceVersionDescriptorPositions::Npm {
            type_prefix_end: type_prefix.map(|p| p.len()).unwrap_or(0),
            alias_package_name,
        }
    })
}

fn url(input: &str) -> Option<SourceVersionDescriptorPositions> {
    tuple((
        opt(tag::<&str, &str, nom::error::Error<&str>>("git+")),
        recognize(tuple((
            tag("http"),
            opt(tag("s")),
            tag("://"),
            take_while(|c| c != '#'),
        ))),
        opt(preceded(tag("#"), rest)),
    ))(input)
    .ok()
    .map(|(_, (git_prefix, url, spec_suffix))| {
        if git_prefix.is_some() || url.ends_with(".git") {
            SourceVersionDescriptorPositions::Git {
                type_prefix_end: git_prefix.map(|p| p.len()).unwrap_or(0),
            }
        } else {
            SourceVersionDescriptorPositions::TarballURL {}
        }
    })
}

fn github(input: &str) -> Option<SourceVersionDescriptorPositions> {
    tuple((
        opt(tag::<&str, &str, ()>("github:")),
        take_while1(|c: char| c.is_ascii_alphanumeric() || c == '-'),
        tag("/"),
        verify(
            take_while1(|c: char| c.is_ascii_alphanumeric() || ['-', '.', '_'].contains(&c)),
            |name: &str| !name.starts_with("."),
        ),
        alt((tag("#"), eof)),
    ))(input)
    .ok()
    .map(
        |(_, (gh_prefix, _, _, _, _))| SourceVersionDescriptorPositions::GitHub {
            type_prefix_end: gh_prefix.map(|p| p.len()).unwrap_or(0),
        },
    )
}

impl SourceVersionDescriptorPositions {
    fn parse(svd: &str) -> Result<Self> {
        npm(svd)
            .or_else(|| url(svd))
            .or_else(|| github(svd))
            .ok_or_else(|| todo!())
    }
}
impl SourceVersionDescriptor {
    pub fn new(svd: String) -> Result<Self> {
        Ok(Self {
            positions: SourceVersionDescriptorPositions::parse(&svd)?,
            inner: svd,
        })
    }
}

impl SourceVersionDescriptorPositions {
    fn aliased_package_name(&self) -> Option<(usize, usize)> {
        match self {
            SourceVersionDescriptorPositions::Npm {
                type_prefix_end,
                alias_package_name: Some(alias),
            } => Some((*type_prefix_end, alias.total_length + type_prefix_end)),
            _ => None,
        }
    }
}

impl SourceVersionDescriptor {
    pub fn is_npm(&self) -> bool {
        matches!(self.positions, SourceVersionDescriptorPositions::Npm { .. })
    }

    /// Package name specified as aliased in the version descriptor.
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionDescriptor;
    /// let svd = SourceVersionDescriptor::new(
    ///     "npm:@chastelock/testcase@^2.1.37".to_string()).unwrap();
    /// assert_eq!(svd.aliased_package_name(), Some("@chastelock/testcase"));
    /// ```
    pub fn aliased_package_name(&self) -> Option<&str> {
        self.positions
            .aliased_package_name()
            .map(|(start, end)| &self.inner[start..end])
    }
}

#[non_exhaustive]
pub enum SourceVersionDescriptorType {
    /// Package from an npm registry.
    Npm,
    /// Arbitrary tarball URL. <https://docs.npmjs.com/cli/v10/configuring-npm/package-json#urls-as-dependencies>
    TarballURL,
    /// Git repository. <https://docs.npmjs.com/cli/v10/configuring-npm/package-json#git-urls-as-dependencies>
    Git,
    /// GitHub repository. No, not the same as [SourceVersionDescriptorType::Git], it's papa's special boy.
    /// <https://docs.npmjs.com/cli/v10/configuring-npm/package-json#git-urls-as-dependencies>
    GitHub,
}

#[cfg(test)]
mod tests {
    use super::SourceVersionDescriptor;
    use crate::error::Result;

    #[test]
    fn npm_svd_basic() -> Result<()> {
        let name = SourceVersionDescriptor::new("^7.0.1".to_string())?;
        assert_eq!(name.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svd_basic_any() -> Result<()> {
        let name = SourceVersionDescriptor::new("*".to_string())?;
        assert_eq!(name.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svd_basic_empty() -> Result<()> {
        let name = SourceVersionDescriptor::new("".to_string())?;
        assert_eq!(name.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svd_alias() -> Result<()> {
        let name = SourceVersionDescriptor::new("npm:chazzwazzer@*".to_string())?;
        assert_eq!(name.aliased_package_name(), Some("chazzwazzer"));
        Ok(())
    }

    #[test]
    fn npm_svd_alias_scoped() -> Result<()> {
        let name = SourceVersionDescriptor::new("@chastelock/testcase@1.0.x".to_string())?;
        assert_eq!(name.aliased_package_name(), Some("@chastelock/testcase"));
        Ok(())
    }
}
