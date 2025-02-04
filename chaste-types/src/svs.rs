// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::ops::{Range, RangeFrom};

pub use nodejs_semver::Range as VersionRange;
use nom::branch::alt;
use nom::bytes::complete::{tag, take_while, take_while1};
use nom::character::complete::digit1;
use nom::combinator::{eof, map_res, opt, recognize, rest, verify};
use nom::sequence::{pair, preceded, terminated};
use nom::Parser;

use crate::error::{Error, Result};
use crate::name::{package_name, PackageNameBorrowed, PackageNamePositions};
use crate::quirks::QuirksMode;

/// Source/version specifier. It is a constraint defined by a specific [`crate::Dependency`],
/// and is used by package managers to choose a specific [`crate::PackageSource`].
///
/// # Example
/// ```
/// # use chaste_types::SourceVersionSpecifier;
/// let svs1 = SourceVersionSpecifier::new(
///     "^1.0.0".to_string()).unwrap();
/// assert!(svs1.is_npm());
///
/// let svs2 = SourceVersionSpecifier::new(
///     "git@codeberg.org:22/selfisekai/chaste.git".to_string()).unwrap();
/// assert!(svs2.is_git());
///
/// let svs3 = SourceVersionSpecifier::new(
///     "https://s.lnl.gay/YMSRcUPRNMxx.tgz".to_string()).unwrap();
/// assert!(svs3.is_tar());
/// ```
#[derive(Debug, Clone)]
pub struct SourceVersionSpecifier {
    inner: String,
    positions: SourceVersionSpecifierPositions,
}

#[derive(Debug, Clone)]
enum SourceVersionSpecifierPositions {
    Npm {
        type_prefix_end: usize,
        alias_package_name: Option<PackageNamePositions>,
    },
    NpmTag {},
    TarballURL {},
    Git {
        type_prefix_end: usize,
        pre_path_sep_offset: Option<usize>,
    },
    GitHub {
        type_prefix_end: usize,
    },
}

fn npm(input: &str) -> Option<SourceVersionSpecifierPositions> {
    (
        opt(tag("npm:")),
        opt(terminated(package_name, tag("@"))),
        map_res(rest, |input: &str| {
            VersionRange::parse(if input.is_empty() { "*" } else { input })
        }),
    )
        .parse(input)
        .ok()
        .map(|(_, (type_prefix, alias_package_name, _range))| {
            SourceVersionSpecifierPositions::Npm {
                type_prefix_end: type_prefix.map(|p| p.len()).unwrap_or(0),
                alias_package_name,
            }
        })
}

fn url(input: &str) -> Option<SourceVersionSpecifierPositions> {
    (
        opt(tag::<&str, &str, nom::error::Error<&str>>("git+")),
        recognize((
            tag("http"),
            opt(tag("s")),
            tag("://"),
            take_while(|c| c != '#'),
        )),
        opt(preceded(tag("#"), rest)),
    )
        .parse(input)
        .ok()
        .map(|(_, (git_prefix, url, _spec_suffix))| {
            if git_prefix.is_some() || url.ends_with(".git") {
                SourceVersionSpecifierPositions::Git {
                    type_prefix_end: git_prefix.map(|p| p.len()).unwrap_or(0),
                    pre_path_sep_offset: None,
                }
            } else {
                SourceVersionSpecifierPositions::TarballURL {}
            }
        })
}

/// This definition is probably too broad
fn ssh(input: &str) -> Option<SourceVersionSpecifierPositions> {
    (
        opt(pair(
            opt(tag::<&str, &str, nom::error::Error<&str>>("git+")),
            tag("ssh://"),
        )),
        take_while(|c| !['/', ':'].contains(&c)),
        opt(preceded(tag(":"), digit1)),
        alt((tag(":"), tag("/"))),
        take_while(|c| c != '#'),
        opt(preceded(tag("#"), rest)),
    )
        .parse(input)
        .ok()
        .and_then(|(_, (prefix, host, port, _sep, url, _spec_suffix))| {
            if prefix.is_some() || url.ends_with(".git") {
                let prefix_len = prefix
                    .map(|(git_prefix, ssh_prefix)| {
                        git_prefix.map(|p| p.len()).unwrap_or(0) + ssh_prefix.len()
                    })
                    .unwrap_or(0);
                Some(SourceVersionSpecifierPositions::Git {
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

fn github(input: &str) -> Option<SourceVersionSpecifierPositions> {
    (
        opt(tag::<&str, &str, ()>("github:")),
        take_while1(|c: char| c.is_ascii_alphanumeric() || c == '-'),
        tag("/"),
        verify(
            take_while1(|c: char| c.is_ascii_alphanumeric() || ['-', '.', '_'].contains(&c)),
            |name: &str| !name.starts_with("."),
        ),
        alt((tag("#"), eof)),
    )
        .parse(input)
        .ok()
        .map(
            |(_, (gh_prefix, _, _, _, _))| SourceVersionSpecifierPositions::GitHub {
                type_prefix_end: gh_prefix.map(|p| p.len()).unwrap_or(0),
            },
        )
}

fn npm_tag(input: &str) -> Option<SourceVersionSpecifierPositions> {
    preceded(
        take_while(|c: char| c.is_ascii() && !c.is_ascii_control()),
        eof::<&str, nom::error::Error<&str>>,
    )
    .parse(input)
    .ok()
    .map(|(_, _)| SourceVersionSpecifierPositions::NpmTag {})
}

impl SourceVersionSpecifierPositions {
    fn parse(svs: &str, quirks: Option<QuirksMode>) -> Result<Self> {
        npm(svs)
            .or_else(|| url(svs))
            .or_else(|| github(svs))
            .or_else(|| {
                ssh(svs).filter(|s| {
                    // in yarn(classic), "ssh://git@github.com:npm/node-semver.git" is interpreted as an npm tag
                    !matches!(quirks, Some(QuirksMode::Yarn(1)))
                        || s.ssh_path_sep()
                            .map(|r| &svs[r])
                            .is_none_or(|sep| sep == "/")
                })
            })
            .or_else(|| npm_tag(svs))
            .ok_or_else(|| Error::InvalidSVS(svs.to_string()))
    }
}
impl SourceVersionSpecifier {
    pub fn new(svs: String) -> Result<Self> {
        Ok(Self {
            positions: SourceVersionSpecifierPositions::parse(&svs, None)?,
            inner: svs,
        })
    }

    pub fn with_quirks(svs: String, quirks: QuirksMode) -> Result<Self> {
        Ok(Self {
            positions: SourceVersionSpecifierPositions::parse(&svs, Some(quirks))?,
            inner: svs,
        })
    }
}

impl SourceVersionSpecifierPositions {
    fn aliased_package_name(&self) -> Option<Range<usize>> {
        match self {
            SourceVersionSpecifierPositions::Npm {
                type_prefix_end,
                alias_package_name: Some(alias),
            } => Some(*type_prefix_end..alias.total_length + type_prefix_end),
            _ => None,
        }
    }
    fn npm_range(&self) -> Option<RangeFrom<usize>> {
        match self {
            SourceVersionSpecifierPositions::Npm {
                type_prefix_end,
                alias_package_name: Some(alias),
            } => Some(alias.total_length + type_prefix_end + 1..),
            SourceVersionSpecifierPositions::Npm {
                type_prefix_end,
                alias_package_name: None,
            } => Some(*type_prefix_end..),
            _ => None,
        }
    }
    fn ssh_path_sep(&self) -> Option<Range<usize>> {
        match self {
            SourceVersionSpecifierPositions::Git {
                pre_path_sep_offset: Some(offset),
                ..
            } => Some(*offset..offset + 1),
            _ => None,
        }
    }
}

impl AsRef<str> for SourceVersionSpecifier {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}
impl PartialEq<str> for SourceVersionSpecifier {
    fn eq(&self, other: &str) -> bool {
        self.inner == other
    }
}

impl SourceVersionSpecifier {
    /// Whether the SVS chooses an npm version range.
    /// This does not include npm tags (see [`SourceVersionSpecifier::is_npm_tag`]).
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs1 = SourceVersionSpecifier::new(
    ///     "^4".to_string()).unwrap();
    /// assert!(svs1.is_npm());
    ///
    /// let svs2 = SourceVersionSpecifier::new(
    ///     "npm:@chastelock/testcase@^2.1.37".to_string()).unwrap();
    /// assert!(svs2.is_npm());
    /// ```
    pub fn is_npm(&self) -> bool {
        matches!(self.positions, SourceVersionSpecifierPositions::Npm { .. })
    }

    /// Whether the SVS chooses an npm tag.
    /// This does not include npm version ranges (see [`SourceVersionSpecifier::is_npm`]).
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs1 = SourceVersionSpecifier::new(
    ///     "latest".to_string()).unwrap();
    /// assert!(svs1.is_npm_tag());
    ///
    /// let svs2 = SourceVersionSpecifier::new(
    ///     "next-11".to_string()).unwrap();
    /// assert!(svs2.is_npm_tag());
    /// ```
    pub fn is_npm_tag(&self) -> bool {
        matches!(
            self.positions,
            SourceVersionSpecifierPositions::NpmTag { .. }
        )
    }

    /// Whether the SVS chooses an arbitrary tarball URL.
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs1 = SourceVersionSpecifier::new(
    ///     "https://s.lnl.gay/YMSRcUPRNMxx.tgz".to_string()).unwrap();
    /// assert!(svs1.is_tar());
    ///
    /// let svs2 = SourceVersionSpecifier::new(
    ///     "https://codeberg.org/libselfisekai/-/packages/npm/@a%2Fempty/0.0.1/files/1152452".to_string()).unwrap();
    /// assert!(svs2.is_tar());
    /// ```
    pub fn is_tar(&self) -> bool {
        matches!(
            self.positions,
            SourceVersionSpecifierPositions::TarballURL { .. }
        )
    }

    /// Whether the SVS chooses a git repository as the source.
    /// This does not include the short-form GitHub slugs (see [`SourceVersionSpecifier::is_github`]).
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs1 = SourceVersionSpecifier::new(
    ///     "ssh://git@github.com/npm/node-semver.git#semver:^7.5.0".to_string()).unwrap();
    /// assert!(svs1.is_git());
    ///
    /// let svs2 = SourceVersionSpecifier::new(
    ///     "https://github.com/isaacs/minimatch.git#v10.0.1".to_string()).unwrap();
    /// assert!(svs2.is_git());
    /// ```
    pub fn is_git(&self) -> bool {
        matches!(self.positions, SourceVersionSpecifierPositions::Git { .. })
    }

    /// Whether the SVS chooses a GitHub.com repository as the source.
    /// This includes the short-form GitHub slugs, and does not include
    /// full-formed Git URLs to github.com (for those, see [`SourceVersionSpecifier::is_git`]).
    ///
    /// While regular Git URLs specify a protocol, in this special case
    /// a package manager can choose between Git over HTTPS, Git over SSH,
    /// and tarball URLs. See [`crate::Package::source`] to find out the real source.
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs1 = SourceVersionSpecifier::new(
    ///     "npm/node-semver#semver:^7.5.0".to_string()).unwrap();
    /// assert!(svs1.is_github());
    ///
    /// let svs2 = SourceVersionSpecifier::new(
    ///     "github:isaacs/minimatch#v10.0.1".to_string()).unwrap();
    /// assert!(svs2.is_github());
    /// ```
    pub fn is_github(&self) -> bool {
        matches!(
            self.positions,
            SourceVersionSpecifierPositions::GitHub { .. }
        )
    }

    /// Package name specified as aliased in the version specifier.
    ///
    /// This is useful for a specific case: npm dependencies defined with a name alias,
    /// e.g. `"lodash": "npm:@chastelock/lodash-fork@^4.1.0"`,
    /// which means that the package `@chastelock/lodash-fork` is available for import
    /// as `lodash`. Normally, there is no rename, and the package's registry name
    /// (available in [`crate::Package::name`]) is used.
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs = SourceVersionSpecifier::new(
    ///     "npm:@chastelock/testcase@^2.1.37".to_string()).unwrap();
    /// assert_eq!(svs.aliased_package_name().unwrap(), "@chastelock/testcase");
    /// ```
    pub fn aliased_package_name(&self) -> Option<PackageNameBorrowed<'_>> {
        match &self.positions {
            SourceVersionSpecifierPositions::Npm {
                alias_package_name: Some(positions),
                ..
            } => Some(PackageNameBorrowed {
                inner: &self.inner[self.positions.aliased_package_name().unwrap()],
                positions,
            }),
            _ => None,
        }
    }

    /// Version range specified by a dependency, as a string.
    ///
    /// For a [VersionRange] object (to compare versions against), check out [SourceVersionSpecifier::npm_range].
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs1 = SourceVersionSpecifier::new(
    ///     "4.2.x".to_string()).unwrap();
    /// assert_eq!(svs1.npm_range_str().unwrap(), "4.2.x");
    ///
    /// let svs2 = SourceVersionSpecifier::new(
    ///     "npm:@chastelock/testcase@^2.1.37".to_string()).unwrap();
    /// assert_eq!(svs2.npm_range_str().unwrap(), "^2.1.37");
    /// ```
    pub fn npm_range_str(&self) -> Option<&str> {
        self.positions.npm_range().map(|r| &self.inner[r])
    }

    /// Version range specified by a dependency, as [VersionRange] object.
    ///
    /// For a string slice, check out [SourceVersionSpecifier::npm_range_str].
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs1 = SourceVersionSpecifier::new(
    ///     "4.2.x".to_string()).unwrap();
    /// assert_eq!(svs1.npm_range_str().unwrap(), "4.2.x");
    ///
    /// let svs2 = SourceVersionSpecifier::new(
    ///     "npm:@chastelock/testcase@^2.1.37".to_string()).unwrap();
    /// assert_eq!(svs2.npm_range_str().unwrap(), "^2.1.37");
    /// ```
    pub fn npm_range(&self) -> Option<VersionRange> {
        self.npm_range_str()
            .map(|r| VersionRange::parse(r).unwrap())
    }

    /// If the dependency source is Git over SSH, this returns the separator used
    /// between the server part and the path. This is either `:` or `/`.
    /// There are quirks in support for these addresses between implementations.
    ///
    /// # Example
    /// ```
    /// # use chaste_types::SourceVersionSpecifier;
    /// let svs1 = SourceVersionSpecifier::new(
    ///     "git@codeberg.org:selfisekai/chaste.git".to_string()).unwrap();
    /// assert_eq!(svs1.ssh_path_sep().unwrap(), ":");
    ///
    /// let svs2 = SourceVersionSpecifier::new(
    ///     "git@codeberg.org:22/selfisekai/chaste.git".to_string()).unwrap();
    /// assert_eq!(svs2.ssh_path_sep().unwrap(), "/");
    /// ```
    pub fn ssh_path_sep(&self) -> Option<&str> {
        self.positions.ssh_path_sep().map(|r| &self.inner[r])
    }
}

#[non_exhaustive]
pub enum SourceVersionSpecifierKind {
    /// Package from an npm registry. Does not include tags (see [`SourceVersionSpecifierKind::NpmTag`])
    Npm,
    /// Named tag from an npm registry, e.g. "latest", "beta".
    NpmTag,
    /// Arbitrary tarball URL. <https://docs.npmjs.com/cli/v10/configuring-npm/package-json#urls-as-dependencies>
    TarballURL,
    /// Git repository. <https://docs.npmjs.com/cli/v10/configuring-npm/package-json#git-urls-as-dependencies>
    Git,
    /// GitHub repository. No, not the same as [`SourceVersionSpecifierKind::Git`], it's papa's special boy.
    /// <https://docs.npmjs.com/cli/v10/configuring-npm/package-json#git-urls-as-dependencies>
    GitHub,
}

impl SourceVersionSpecifier {
    pub fn kind(&self) -> SourceVersionSpecifierKind {
        match &self.positions {
            SourceVersionSpecifierPositions::Npm { .. } => SourceVersionSpecifierKind::Npm,
            SourceVersionSpecifierPositions::NpmTag { .. } => SourceVersionSpecifierKind::NpmTag,
            SourceVersionSpecifierPositions::TarballURL { .. } => {
                SourceVersionSpecifierKind::TarballURL
            }
            SourceVersionSpecifierPositions::Git { .. } => SourceVersionSpecifierKind::Git,
            SourceVersionSpecifierPositions::GitHub { .. } => SourceVersionSpecifierKind::GitHub,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SourceVersionSpecifier;
    use crate::error::Result;

    #[test]
    fn npm_svs_basic() -> Result<()> {
        let svs = SourceVersionSpecifier::new("^7.0.1".to_string())?;
        assert!(svs.is_npm());
        assert_eq!(svs.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svs_basic_any() -> Result<()> {
        let svs = SourceVersionSpecifier::new("*".to_string())?;
        assert!(svs.is_npm());
        assert_eq!(svs.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svs_basic_empty() -> Result<()> {
        let svs = SourceVersionSpecifier::new("".to_string())?;
        assert!(svs.is_npm());
        assert_eq!(svs.aliased_package_name(), None);
        Ok(())
    }

    #[test]
    fn npm_svs_alias() -> Result<()> {
        let svs = SourceVersionSpecifier::new("npm:chazzwazzer@*".to_string())?;
        assert!(svs.is_npm());
        assert_eq!(svs.aliased_package_name().unwrap(), "chazzwazzer");
        Ok(())
    }

    #[test]
    fn npm_svs_alias_scoped() -> Result<()> {
        let svs = SourceVersionSpecifier::new("@chastelock/testcase@1.0.x".to_string())?;
        assert!(svs.is_npm());
        assert_eq!(svs.aliased_package_name().unwrap(), "@chastelock/testcase");
        Ok(())
    }

    #[test]
    fn npm_svs_tag() -> Result<()> {
        let svs = SourceVersionSpecifier::new("next-11".to_string())?;
        assert!(svs.is_npm_tag());
        Ok(())
    }

    #[test]
    fn tar_svs() -> Result<()> {
        let svs = SourceVersionSpecifier::new("https://example.com/not-a-git-repo".to_string())?;
        assert!(svs.is_tar());
        Ok(())
    }

    #[test]
    fn git_http_svs_unspecified() -> Result<()> {
        let svs =
            SourceVersionSpecifier::new("https://codeberg.org/selfisekai/chaste.git".to_string())?;
        assert!(svs.is_git());
        Ok(())
    }

    #[test]
    fn git_http_svs_unspecified_prefixed() -> Result<()> {
        let svs =
            SourceVersionSpecifier::new("git+https://codeberg.org/selfisekai/chaste".to_string())?;
        assert!(svs.is_git());
        Ok(())
    }

    #[test]
    fn git_http_svs_tag() -> Result<()> {
        let svs = SourceVersionSpecifier::new(
            "https://github.com/npm/node-semver.git#v7.6.3".to_string(),
        )?;
        assert!(svs.is_git());
        Ok(())
    }

    #[test]
    fn git_http_svs_semver() -> Result<()> {
        let svs = SourceVersionSpecifier::new(
            "https://github.com/npm/node-semver.git#semver:^7.5.0".to_string(),
        )?;
        assert!(svs.is_git());
        Ok(())
    }

    #[test]
    fn git_ssh_svs_unspecified() -> Result<()> {
        let svs =
            SourceVersionSpecifier::new("git@codeberg.org:selfisekai/chaste.git".to_string())?;
        assert!(svs.is_git());
        assert_eq!(svs.ssh_path_sep(), Some(":"));
        Ok(())
    }

    #[test]
    fn git_ssh_svs_unspecified_prefixed() -> Result<()> {
        let svs = SourceVersionSpecifier::new(
            "git+ssh://git@codeberg.org:selfisekai/chaste".to_string(),
        )?;
        assert!(svs.is_git());
        assert_eq!(svs.ssh_path_sep(), Some(":"));
        Ok(())
    }

    #[test]
    fn git_ssh_svs_tag() -> Result<()> {
        let svs =
            SourceVersionSpecifier::new("git@github.com:npm/node-semver.git#v7.6.3".to_string())?;
        assert!(svs.is_git());
        assert_eq!(svs.ssh_path_sep(), Some(":"));
        Ok(())
    }

    #[test]
    fn git_ssh_svs_semver() -> Result<()> {
        let svs = SourceVersionSpecifier::new(
            "git@github.com:npm/node-semver.git#semver:^7.5.0".to_string(),
        )?;
        assert!(svs.is_git());
        assert_eq!(svs.ssh_path_sep(), Some(":"));
        Ok(())
    }

    #[test]
    fn github_svs_unspecified() -> Result<()> {
        let svs = SourceVersionSpecifier::new("npm/node-semver".to_string())?;
        assert!(svs.is_github());
        Ok(())
    }

    #[test]
    fn github_svs_unspecified_prefixed() -> Result<()> {
        let svs = SourceVersionSpecifier::new("github:npm/node-semver".to_string())?;
        assert!(svs.is_github());
        Ok(())
    }

    #[test]
    fn github_svs_tag() -> Result<()> {
        let svs = SourceVersionSpecifier::new("npm/node-semver#7.5.1".to_string())?;
        assert!(svs.is_github());
        Ok(())
    }

    #[test]
    fn github_svs_semver() -> Result<()> {
        let svs = SourceVersionSpecifier::new("npm/node-semver#semver:^7.5.0".to_string())?;
        assert!(svs.is_github());
        Ok(())
    }
}
