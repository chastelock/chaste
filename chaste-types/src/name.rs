// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::{cmp, fmt};

use crate::error::{PackageNameError, Result};
use crate::misc::partial_eq_field;

#[derive(Debug, PartialEq, Eq, Clone)]
struct PackageNamePositions {
    scope_end: Option<usize>,
    total_length: usize,
}

impl PackageNamePositions {
    fn parse(input: &str) -> Result<Self, PackageNameError> {
        let mut chars = input.chars().enumerate().peekable();
        let mut scope_end = None;
        if let Some((0, '@')) = chars.peek() {
            let scope_at = chars.next();
            debug_assert_eq!(scope_at, Some((0, '@')));
            match chars.next().ok_or(PackageNameError::UnexpectedEnd {
                #[cfg(feature = "miette")]
                at: (1, 0).into(),
            })? {
                (_, 'a'..='z' | '0'..='9') => {}
                #[allow(unused_variables)]
                (pos, char) => {
                    return Err(PackageNameError::InvalidCharacter {
                        char: char,
                        #[cfg(feature = "miette")]
                        at: (pos, 1).into(),
                    });
                }
            }
            while let Some((pos, char)) = chars.next() {
                match char {
                    'a'..='z' | '0'..='9' | '.' | '-' | '_' => {}
                    '/' => {
                        scope_end = Some(pos);
                        break;
                    }
                    // TODO
                    _ => {
                        return Err(PackageNameError::InvalidCharacter {
                            char: char,
                            #[cfg(feature = "miette")]
                            at: (pos, 1).into(),
                        });
                    }
                }
            }
        }
        match chars.next().ok_or(PackageNameError::UnexpectedEnd {
            #[cfg(feature = "miette")]
            at: (input.len(), 0).into(),
        })? {
            (_, 'a'..='z' | '0'..='9') => {}
            #[allow(unused_variables)]
            (pos, char) => {
                return Err(PackageNameError::InvalidCharacter {
                    char,
                    #[cfg(feature = "miette")]
                    at: (pos, 1).into(),
                });
            }
        }
        #[allow(unused_variables)]
        while let Some((pos, char)) = chars.next() {
            match char {
                'a'..='z' | '0'..='9' | '.' | '-' | '_' => {}
                // TODO
                _ => {
                    return Err(PackageNameError::InvalidCharacter {
                        char: char,
                        #[cfg(feature = "miette")]
                        at: (pos, 1).into(),
                    });
                }
            }
        }

        Ok(PackageNamePositions {
            scope_end,
            total_length: input.len(),
        })
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

partial_eq_field!(PackageName, inner, String);
partial_eq_field!(PackageName, inner, &str);

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
    pub fn new(name: String) -> Result<Self, PackageNameError> {
        Ok(Self {
            positions: PackageNamePositions::parse(&name)?,
            inner: name,
        })
    }

    option_segment!(scope);
    option_segment!(scope_prefix);
    option_segment!(scope_name);
    required_segment!(name_rest);
}

impl TryFrom<String> for PackageName {
    type Error = PackageNameError;

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
impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str {
        &self.inner
    }
}
impl PartialOrd for PackageName {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.inner.cmp(&other.inner))
    }
}
impl Ord for PackageName {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.inner.cmp(&other.inner)
    }
}

#[cfg(test)]
mod tests {
    use crate::error::PackageNameError;

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
            ($name:expr, $err:expr) => {
                assert_eq!(PackageName::new($name.to_string()), Err($err));
            };
        }
        assert_name_error!(
            "",
            PackageNameError::UnexpectedEnd {
                #[cfg(feature = "miette")]
                at: (0, 0).into(),
            }
        );
        assert_name_error!(
            "ą",
            PackageNameError::InvalidCharacter {
                char: 'ą',
                #[cfg(feature = "miette")]
                at: (0, 1).into(),
            }
        );
        assert_name_error!(
            ".bin",
            PackageNameError::InvalidCharacter {
                char: '.',
                #[cfg(feature = "miette")]
                at: (0, 1).into(),
            }
        );
        assert_name_error!(
            "a/",
            PackageNameError::InvalidCharacter {
                char: '/',
                #[cfg(feature = "miette")]
                at: (1, 1).into(),
            }
        );
        assert_name_error!(
            "a@a/a",
            PackageNameError::InvalidCharacter {
                char: '@',
                #[cfg(feature = "miette")]
                at: (1, 1).into(),
            }
        );
        assert_name_error!(
            "@",
            PackageNameError::UnexpectedEnd {
                #[cfg(feature = "miette")]
                at: (1, 0).into(),
            }
        );
        assert_name_error!(
            "@a",
            PackageNameError::UnexpectedEnd {
                #[cfg(feature = "miette")]
                at: (2, 0).into(),
            }
        );
        assert_name_error!(
            "@a/",
            PackageNameError::UnexpectedEnd {
                #[cfg(feature = "miette")]
                at: (3, 0).into(),
            }
        );
        assert_name_error!(
            "/",
            PackageNameError::InvalidCharacter {
                char: '/',
                #[cfg(feature = "miette")]
                at: (0, 1).into(),
            }
        );
        assert_name_error!(
            "@/a",
            PackageNameError::InvalidCharacter {
                char: '/',
                #[cfg(feature = "miette")]
                at: (1, 1).into(),
            }
        );
        assert_name_error!(
            "@/",
            PackageNameError::InvalidCharacter {
                char: '/',
                #[cfg(feature = "miette")]
                at: (1, 1).into(),
            }
        );
    }
}
