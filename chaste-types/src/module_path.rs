// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::sync::LazyLock;

use crate::error::{Error, Result};
use crate::name::{package_name, PackageName, PackageNameBorrowed, PackageNamePositions};

pub static ROOT_MODULE_PATH: LazyLock<ModulePath> =
    LazyLock::new(|| ModulePath::new("".to_string()).unwrap());

#[derive(Debug, PartialEq, Eq, Clone)]
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

#[derive(Debug, Clone)]
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
                    return Err(Error::InvalidModulePath(value.to_string()));
                }
                "node_modules" => {
                    inside_node_modules = true;
                    segments.push(ModulePathSegmentInternal::NodeModules(end_idx));
                }
                seg if inside_node_modules && !inside_scoped && seg.starts_with("@") => {
                    inside_scoped = true;
                }
                _ if inside_node_modules => {
                    let last_seg = segments.last().unwrap();
                    if matches!(last_seg, ModulePathSegmentInternal::PackageName(..)) {
                        return Err(Error::InvalidModulePath(value.to_string()));
                    }
                    let start_idx = last_seg.end_idx() + 1;
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
        if inside_scoped
            || segments
                .last()
                .is_some_and(|s| matches!(s, ModulePathSegmentInternal::NodeModules(_)))
        {
            return Err(Error::InvalidModulePath(value.to_string()));
        }

        debug_assert_eq!(
            segments.last().map(|s| s.end_idx()).unwrap_or(0),
            value.len()
        );

        Ok(Self {
            inner: value,
            segments,
        })
    }

    pub fn implied_package_name(&self) -> Option<PackageName> {
        let iter = self.iter();
        match iter.last() {
            Some(ModulePathSegment::PackageName(pn)) => Some(pn.to_owned()),
            Some(ModulePathSegment::Arbitrary(a)) => match self.segments.len() {
                1 => PackageName::new(a.to_string()).ok(),
                2 if self.inner.starts_with("@") => PackageName::new(self.inner.clone()).ok(),
                0 => unreachable!(),
                len => {
                    let scope_start = self.segments.get(len - 2).unwrap().end_idx() + 1;
                    PackageName::new(self.inner[scope_start..].to_string()).ok()
                }
            },
            Some(ModulePathSegment::NodeModules(_)) => unreachable!(),
            None => None,
        }
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
impl<'a> AsRef<str> for ModulePathSegment<'a> {
    fn as_ref(&self) -> &str {
        match self {
            ModulePathSegment::Arbitrary(i) => i,
            ModulePathSegment::NodeModules(i) => i,
            ModulePathSegment::PackageName(package_name_borrowed) => package_name_borrowed.as_ref(),
        }
    }
}
impl<'a> PartialOrd for ModulePathSegment<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl<'a> Ord for ModulePathSegment<'a> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
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
        let seg_int = self.inner.segments.get(seg_idx)?;
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
            inner: self,
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

    #[test]
    fn bs_paths() -> Result<()> {
        fn invalid(input: &str) {
            assert!(ModulePath::new(input.to_string()).is_err());
        }
        invalid("/");
        invalid("a/");
        invalid("node_modules");
        invalid("node_modules/@chastelock/testcase/something/deeper");
        invalid("node_modules/@chastelock");
        Ok(())
    }
}
