// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use crate::error::{Error, Result};
use crate::name::{package_name, PackageNameBorrowed, PackageNamePositions};

#[derive(Debug, PartialEq, Eq)]
enum ModulePathSegmentInternal {
    Arbitrary(usize),
    NodeModules(usize),
    PackageName(usize, PackageNamePositions),
}

impl ModulePathSegmentInternal {
    fn end_idx(&self) -> usize {
        match self {
            Self::Arbitrary(i) => *i,
            Self::NodeModules(i) => *i,
            Self::PackageName(i, _) => *i,
        }
    }
}

pub struct ModulePath {
    inner: String,
    segments: Vec<ModulePathSegmentInternal>,
}

impl ModulePath {
    pub fn new(value: String) -> Result<Self> {
        let mut segments = Vec::new();
        let mut end_idx = 0usize;
        let mut inside_node_modules = false;
        let mut inside_scoped = false;
        for (i, segment) in value.split("/").enumerate() {
            end_idx += segment.len();
            if i != 0 {
                end_idx += 1;
            }
            match segment {
                "" => {
                    // Empty value is a special case for root path
                    if value.is_empty() {
                        break;
                    }
                    // TODO: Add new error type
                    todo!();
                }
                "node_modules" => {
                    inside_node_modules = true;
                    segments.push(ModulePathSegmentInternal::NodeModules(end_idx));
                }
                seg if inside_node_modules && !inside_scoped && seg.starts_with("@") => {
                    inside_scoped = true;
                }
                _ if inside_node_modules => {
                    let start_idx = segments.last().unwrap().end_idx() + 1;
                    let pn_str = &value[start_idx..end_idx];
                    let (remaining, pn_positions) = package_name(pn_str)
                        .map_err(|_| Error::InvalidPackageName(pn_str.to_string()))?;
                    if !remaining.is_empty() {
                        return Err(Error::InvalidPackageName(pn_str.to_string()));
                    }
                    segments.push(ModulePathSegmentInternal::PackageName(
                        end_idx,
                        pn_positions,
                    ));

                    inside_scoped = false;
                }
                _ => {
                    segments.push(ModulePathSegmentInternal::Arbitrary(end_idx));
                }
            }
        }

        Ok(Self {
            inner: value,
            segments,
        })
    }
}

impl AsRef<str> for ModulePath {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ModulePathSegment<'a> {
    Arbitrary(&'a str),
    NodeModules(&'a str),
    PackageName(PackageNameBorrowed<'a>),
}

pub struct ModulePathIter<'a> {
    inner: &'a ModulePath,
    idx: usize,
}

impl<'a> Iterator for ModulePathIter<'a> {
    type Item = ModulePathSegment<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let seg_idx = self.idx;
        self.idx += 1;
        let Some(seg_int) = self.inner.segments.get(seg_idx) else {
            return None;
        };
        let start_idx = if seg_idx == 0 {
            0
        } else {
            self.inner
                .segments
                .get(seg_idx - 1)
                .map(|s| s.end_idx() + 1)
                .unwrap()
        };
        let slic = &self.inner.inner[start_idx..seg_int.end_idx()];
        Some(match seg_int {
            ModulePathSegmentInternal::Arbitrary(_) => ModulePathSegment::Arbitrary(slic),
            ModulePathSegmentInternal::NodeModules(_) => ModulePathSegment::NodeModules(slic),
            ModulePathSegmentInternal::PackageName(_, package_name_positions) => {
                ModulePathSegment::PackageName(PackageNameBorrowed {
                    inner: slic,
                    positions: package_name_positions,
                })
            }
        })
    }
}

impl ModulePath {
    pub fn iter(&self) -> ModulePathIter {
        ModulePathIter {
            inner: &self,
            idx: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::LazyLock;

    use crate::error::Result;
    use crate::name::PackageName;

    use super::{ModulePath, ModulePathSegment};

    static SEMVER_PN: LazyLock<PackageName> =
        LazyLock::new(|| PackageName::new("semver".to_string()).unwrap());
    static TESTCASE_PN: LazyLock<PackageName> =
        LazyLock::new(|| PackageName::new("@chastelock/testcase".to_string()).unwrap());

    #[test]
    fn basic_mod() -> Result<()> {
        let path = ModulePath::new("node_modules/semver".to_string())?;
        let mut segments = path.iter();
        assert_eq!(
            segments.next(),
            Some(ModulePathSegment::NodeModules("node_modules"))
        );
        assert_eq!(
            segments.next(),
            Some(ModulePathSegment::PackageName(SEMVER_PN.as_borrowed()))
        );
        assert_eq!(segments.next(), None);

        Ok(())
    }

    #[test]
    fn basic_mod_scoped() -> Result<()> {
        let path = ModulePath::new("node_modules/@chastelock/testcase".to_string())?;
        let mut segments = path.iter();
        assert_eq!(
            segments.next(),
            Some(ModulePathSegment::NodeModules("node_modules"))
        );
        assert_eq!(
            segments.next(),
            Some(ModulePathSegment::PackageName(TESTCASE_PN.as_borrowed()))
        );
        assert_eq!(segments.next(), None);

        Ok(())
    }

    #[test]
    fn empty() -> Result<()> {
        let path = ModulePath::new("".to_string())?;
        let mut segments = path.iter();
        assert_eq!(segments.next(), None);

        Ok(())
    }

    #[test]
    fn mod_inside_workspace_member() -> Result<()> {
        let path =
            ModulePath::new("arbitrary/prefix/node_modules/@chastelock/testcase".to_string())?;
        let mut segments = path.iter();
        assert_eq!(
            segments.next(),
            Some(ModulePathSegment::Arbitrary("arbitrary"))
        );
        assert_eq!(
            segments.next(),
            Some(ModulePathSegment::Arbitrary("prefix"))
        );
        assert_eq!(
            segments.next(),
            Some(ModulePathSegment::NodeModules("node_modules"))
        );
        assert_eq!(
            segments.next(),
            Some(ModulePathSegment::PackageName(TESTCASE_PN.as_borrowed()))
        );
        assert_eq!(segments.next(), None);

        Ok(())
    }
}
