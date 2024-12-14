// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use nom::branch::alt;
use nom::bytes::complete::{tag, take_while, take_while1};
use nom::character::complete::digit1;
use nom::combinator::{eof, opt, recognize, rest, verify};
use nom::sequence::{pair, preceded, tuple};

use crate::error::{Result, SVDError};
use crate::name::{PackageNameBorrowed, PackageNamePositions};

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
        pre_path_sep_offset: Option<usize>,
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
        |input: &str| {
            // "" is a valid version specifier, but Range does not accept it.
            // Override it to a working equivalent.
            let range = nodejs_semver::Range::parse(if input.is_empty() { "*" } else { input })
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
    .map(|(_, (git_prefix, url, _spec_suffix))| {
        if git_prefix.is_some() || url.ends_with(".git") {
            SourceVersionDescriptorPositions::Git {
                type_prefix_end: git_prefix.map(|p| p.len()).unwrap_or(0),
                pre_path_sep_offset: None,
            }
        } else {
            SourceVersionDescriptorPositions::TarballURL {}
        }
    })
}

/// This definition is probably too broad
fn ssh(input: &str) -> Option<SourceVersionDescriptorPositions> {
    tuple((
        opt(pair(
            opt(tag::<&str, &str, nom::error::Error<&str>>("git+")),
            tag("ssh://"),
        )),
        take_while(|c| !['/', ':'].contains(&c)),
        opt(preceded(tag(":"), digit1)),
        alt((tag(":"), tag("/"))),
        take_while(|c| c != '#'),
        opt(preceded(tag("#"), rest)),
    ))(input)
    .ok()
    .and_then(|(_, (prefix, host, port, _sep, url, _spec_suffix))| {
        if prefix.is_some() || url.ends_with(".git") {
            let prefix_len = prefix
                .map(|(git_prefix, ssh_prefix)| {
                    git_prefix.map(|p| p.len()).unwrap_or(0) + ssh_prefix.len()
                })
                .unwrap_or(0);
            Some(SourceVersionDescriptorPositions::Git {
                type_prefix_end: prefix_len,
                pre_path_sep_offset: Some(
                    prefix_len + host.len() + port.map(|p| p.len() + 1).unwrap_or(0),
                ),
            })
        } else {
            None
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
    fn parse(svd: &str) -> Result<Self, SVDError> {
        npm(svd)
            .or_else(|| url(svd))
            .or_else(|| github(svd))
            .or_else(|| ssh(svd))
            .ok_or_else(|| SVDError::UnrecognizedType(svd.to_string()))
    }
}
impl SourceVersionDescriptor {
    pub fn new(svd: String) -> Result<Self, SVDError> {
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
    fn ssh_path_sep(&self) -> Option<(usize, usize)> {
        match self {
            SourceVersionDescriptorPositions::Git {
                pre_path_sep_offset: Some(offset),
                ..
            } => Some((*offset, offset + 1)),
            _ => None,
        }
    }
}

impl SourceVersionDescriptor {
    pub fn is_npm(&self) -> bool {
        matches!(self.positions, SourceVersionDescriptorPositions::Npm { .. })
    }

    pub fn is_tar(&self) -> bool {
        matches!(
            self.positions,
            SourceVersionDescriptorPositions::TarballURL { .. }
        )
    }

    pub fn is_git(&self) -> bool {
        matches!(self.positions, SourceVersionDescriptorPositions::Git { .. })
    }

    pub fn is_github(&self) -> bool {
        matches!(
            self.positions,
            SourceVersionDescriptorPositions::GitHub { .. }
        )
    }

    /// Package name specified as aliased in the version descriptor.
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionDescriptor;
    /// let svd = SourceVersionDescriptor::new(
    ///     "npm:@chastelock/testcase@^2.1.37".to_string()).unwrap();
    /// assert_eq!(svd.aliased_package_name().unwrap(), "@chastelock/testcase");
    /// ```
    pub fn aliased_package_name(&self) -> Option<PackageNameBorrowed<'_>> {
        match &self.positions {
            SourceVersionDescriptorPositions::Npm {
                alias_package_name: Some(positions),
                ..
            } => {
                let (start, end) = self.positions.aliased_package_name().unwrap();
                Some(PackageNameBorrowed {
                    inner: &self.inner[start..end],
                    positions,
                })
            }
            _ => None,
        }
    }

    pub fn ssh_path_sep(&self) -> Option<&str> {
        self.positions
            .ssh_path_sep()
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
        let svd = SourceVersionDescriptor::new("^7.0.1".to_string())?;
        assert!(svd.is_npm());
        assert_eq!(svd.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svd_basic_any() -> Result<()> {
        let svd = SourceVersionDescriptor::new("*".to_string())?;
        assert!(svd.is_npm());
        assert_eq!(svd.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svd_basic_empty() -> Result<()> {
        let svd = SourceVersionDescriptor::new("".to_string())?;
        assert!(svd.is_npm());
        assert_eq!(svd.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svd_alias() -> Result<()> {
        let svd = SourceVersionDescriptor::new("npm:chazzwazzer@*".to_string())?;
        assert!(svd.is_npm());
        assert_eq!(svd.aliased_package_name().unwrap(), "chazzwazzer");
        Ok(())
    }

    #[test]
    fn npm_svd_alias_scoped() -> Result<()> {
        let svd = SourceVersionDescriptor::new("@chastelock/testcase@1.0.x".to_string())?;
        assert!(svd.is_npm());
        assert_eq!(svd.aliased_package_name().unwrap(), "@chastelock/testcase");
        Ok(())
    }

    #[test]
    fn tar_svd() -> Result<()> {
        let svd = SourceVersionDescriptor::new("https://example.com/not-a-git-repo".to_string())?;
        assert!(svd.is_tar());
        Ok(())
    }

    #[test]
    fn git_http_svd_unspecified() -> Result<()> {
        let svd =
            SourceVersionDescriptor::new("https://codeberg.org/selfisekai/chaste.git".to_string())?;
        assert!(svd.is_git());
        Ok(())
    }

    #[test]
    fn git_http_svd_unspecified_prefixed() -> Result<()> {
        let svd =
            SourceVersionDescriptor::new("git+https://codeberg.org/selfisekai/chaste".to_string())?;
        assert!(svd.is_git());
        Ok(())
    }

    #[test]
    fn git_http_svd_tag() -> Result<()> {
        let svd = SourceVersionDescriptor::new(
            "https://github.com/npm/node-semver.git#v7.6.3".to_string(),
        )?;
        assert!(svd.is_git());
        Ok(())
    }

    #[test]
    fn git_http_svd_semver() -> Result<()> {
        let svd = SourceVersionDescriptor::new(
            "https://github.com/npm/node-semver.git#semver:^7.5.0".to_string(),
        )?;
        assert!(svd.is_git());
        Ok(())
    }

    #[test]
    fn git_ssh_svd_unspecified() -> Result<()> {
        let svd =
            SourceVersionDescriptor::new("git@codeberg.org:selfisekai/chaste.git".to_string())?;
        assert!(svd.is_git());
        assert_eq!(svd.ssh_path_sep(), Some(":"));
        Ok(())
    }

    #[test]
    fn git_ssh_svd_unspecified_prefixed() -> Result<()> {
        let svd = SourceVersionDescriptor::new(
            "git+ssh://git@codeberg.org:selfisekai/chaste".to_string(),
        )?;
        assert!(svd.is_git());
        assert_eq!(svd.ssh_path_sep(), Some(":"));
        Ok(())
    }

    #[test]
    fn git_ssh_svd_tag() -> Result<()> {
        let svd =
            SourceVersionDescriptor::new("git@github.com:npm/node-semver.git#v7.6.3".to_string())?;
        assert!(svd.is_git());
        assert_eq!(svd.ssh_path_sep(), Some(":"));
        Ok(())
    }

    #[test]
    fn git_ssh_svd_semver() -> Result<()> {
        let svd = SourceVersionDescriptor::new(
            "git@github.com:npm/node-semver.git#semver:^7.5.0".to_string(),
        )?;
        assert!(svd.is_git());
        assert_eq!(svd.ssh_path_sep(), Some(":"));
        Ok(())
    }

    #[test]
    fn github_svd_unspecified() -> Result<()> {
        let svd = SourceVersionDescriptor::new("npm/node-semver".to_string())?;
        assert!(svd.is_github());
        Ok(())
    }

    #[test]
    fn github_svd_unspecified_prefixed() -> Result<()> {
        let svd = SourceVersionDescriptor::new("github:npm/node-semver".to_string())?;
        assert!(svd.is_github());
        Ok(())
    }

    #[test]
    fn github_svd_tag() -> Result<()> {
        let svd = SourceVersionDescriptor::new("npm/node-semver#7.5.1".to_string())?;
        assert!(svd.is_github());
        Ok(())
    }

    #[test]
    fn github_svd_semver() -> Result<()> {
        let svd = SourceVersionDescriptor::new("npm/node-semver#semver:^7.5.0".to_string())?;
        assert!(svd.is_github());
        Ok(())
    }
}
