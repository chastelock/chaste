// SPDX-FileCopyrightText: 2026 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::BTreeMap;

use nom::branch::alt;
use nom::bytes::complete::{is_not, tag, take_till1};
use nom::combinator::{map, opt, rest};
use nom::sequence::{preceded, terminated};
use nom::{IResult, Parser as _};

use chaste_types::package_name_str;

use crate::error::Result;
use crate::Error;

#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ResolutionSelector<'a> {
    pub name: &'a str,
    pub svs: Option<&'a str>,
}

fn specifier<'a>(input: &'a str) -> IResult<&'a str, &'a str> {
    take_till1(|c| c == '/' || c == '@').parse(input)
}

fn selector<'a>(input: &'a str) -> IResult<&'a str, ResolutionSelector<'a>> {
    (package_name_str, opt(preceded(tag("@"), specifier)))
        .parse(input)
        .map(|(i, (name, svs))| (i, ResolutionSelector { name, svs }))
}

pub(crate) struct ResolutionKey<'a> {
    /// If Some, resolution is only applying when the dependency is requested
    /// by the specific parent package
    pub parent: Option<ResolutionSelector<'a>>,
    pub selector: ResolutionSelector<'a>,
}

fn resolution_key<'a>(input: &'a str) -> Result<ResolutionKey<'a>> {
    match (selector, opt(preceded(tag("/"), selector))).parse(input) {
        Ok(("", (parent, Some(selector)))) => Ok(ResolutionKey {
            parent: Some(parent),
            selector,
        }),
        Ok(("", (selector, None))) => Ok(ResolutionKey {
            parent: None,
            selector,
        }),
        Ok(_) | Err(_) => Err(Error::InvalidResolution(input.to_string())),
    }
}

impl<'a> ResolutionKey<'a> {
    #[inline]
    pub fn parse(input: &'a str) -> Result<ResolutionKey<'a>> {
        resolution_key(input)
    }
}

#[derive(Default)]
pub(crate) struct Resolutions<'a> {
    /// THESE ARE THE OTHER WAY AROUND. Parent is second.
    store: BTreeMap<(ResolutionSelector<'a>, Option<ResolutionSelector<'a>>), &'a str>,
}

impl<'a> Resolutions<'a> {
    pub fn new() -> Resolutions<'a> {
        Resolutions::default()
    }

    pub fn insert(&mut self, key: &'a str, value: &'a str) -> Result<()> {
        let rk = ResolutionKey::parse(key)?;
        let had_value = self.store.insert((rk.selector, rk.parent), value).is_some();
        debug_assert!(!had_value, "Duplicate resolution key");
        Ok(())
    }

    pub fn find<P>(&self, selector: (&str, &str), parent: P) -> Option<&'a str>
    where
        P: Fn() -> &'a [(&'a str, &'a str)],
    {
        let range = self.store.range(
            (
                ResolutionSelector {
                    name: selector.0,
                    svs: None,
                },
                None,
            )..,
        );
        for ((sel, key_parent), value) in range {
            if sel.name != selector.0 {
                break;
            }
            if sel.svs.is_none_or(|s| is_same_svs(s, selector.1)) {
                match key_parent {
                    Some(kp)
                        if parent().iter().any(|p| {
                            kp.name == p.0 && kp.svs.is_none_or(|s| is_same_svs(s, p.1))
                        }) =>
                    {
                        return Some(value)
                    }
                    None => return Some(value),
                    // Failed the if above
                    Some(_) => {}
                }
            }
        }
        None
    }
}

macro_rules! if_only {
    ($expr:expr) => {
        if $expr {
            return true;
        }
    };
}
/// `one` should be as specified by user, `other` should be as put processed into lockfile.
/// E.g. what is "^4.20" in package.json should be on the left, and what is "npm:^4.20" in the lockfile,
/// should be on the right.
pub(crate) fn is_same_svs(one: &str, other: &str) -> bool {
    if_only!(one == other);
    if_only!(Some(one) == other.strip_prefix("npm:"));
    // The SVS can have additional parameters added.
    // "name@patch:name@0.1.0#./file.patch::locator=%40chastelock%2Ftestcase%40workspace%3A."
    if_only!(Some(one) == other.rsplit_once("::").map(|s| s.0));

    false
}

pub(crate) fn is_same_svs_zpm(one: &str, other: &str) -> bool {
    if_only!(is_same_svs(one, other));
    if_only!((
        preceded(
            alt((
                tag::<&str, &str, ()>("https://github.com/"),
                tag("http://github.com/"),
                tag("ssh://git@github.com/"),
            )),
            terminated(is_not("/"), tag("/")),
        ),
        map(is_not("#"), |i: &str| i.strip_suffix(".git").unwrap_or(i)),
        rest,
    )
        .parse(one)
        .is_ok_and(|o| Some(o)
            == (
                preceded(
                    tag::<&str, &str, ()>("github:"),
                    terminated(is_not("/"), tag("/")),
                ),
                is_not("#"),
                rest
            )
                .parse(other)
                .ok()));
    if_only!(Some(one) == other.strip_prefix("github:"));
    if_only!((
        preceded(
            opt(alt((
                tag::<&str, &str, ()>("https://github.com/"),
                tag("http://github.com/"),
                tag("ssh://git@github.com/"),
                tag("github:"),
            ))),
            terminated(is_not("/"), tag("/")),
        ),
        map(is_not("#"), |i: &str| i.strip_suffix(".git").unwrap_or(i)),
        preceded(tag("#semver:"), rest),
    )
        .parse(one)
        .is_ok_and(|o| Some(o)
            == (
                preceded(
                    alt((
                        tag::<&str, &str, ()>("github:"),
                        tag("ssh://git@github.com/"),
                    )),
                    terminated(is_not("/"), tag("/")),
                ),
                map(is_not("#"), |i: &str| i.strip_suffix(".git").unwrap_or(i)),
                preceded(tag("#semver="), rest),
            )
                .parse(other)
                .ok()));

    false
}

#[cfg(test)]
mod tests {
    use super::{ResolutionKey, Resolutions};
    use crate::error::Result;

    #[test]
    fn test_parse_resolution_keys() -> Result<()> {
        fn compare(input: &str, expected: (Option<(&str, Option<&str>)>, (&str, Option<&str>))) {
            assert_eq!(
                ResolutionKey::parse(input)
                    .map(|rk| {
                        (
                            rk.parent.map(|s| (s.name, s.svs)),
                            (rk.selector.name, rk.selector.svs),
                        )
                    })
                    .unwrap(),
                expected
            );
        }

        compare("lodash", (None, ("lodash", None)));
        compare(
            "@chastelock/testcase",
            (None, ("@chastelock/testcase", None)),
        );
        compare(
            "@yarnpkg/core@npm:^4.5",
            (None, ("@yarnpkg/core", Some("npm:^4.5"))),
        );
        compare(
            "@yarnpkg/core@npm:^4.5/lodash",
            (Some(("@yarnpkg/core", Some("npm:^4.5"))), ("lodash", None)),
        );
        compare(
            "@yarnpkg/core@npm:^4.5/@scope/lodash",
            (
                Some(("@yarnpkg/core", Some("npm:^4.5"))),
                ("@scope/lodash", None),
            ),
        );
        compare(
            "@yarnpkg/core@npm:^4.5/@scope/lodash@^1",
            (
                Some(("@yarnpkg/core", Some("npm:^4.5"))),
                ("@scope/lodash", Some("^1")),
            ),
        );

        Ok(())
    }

    #[test]
    fn test_evaluate_resolutions() -> Result<()> {
        let mut resolutions = Resolutions::new();
        resolutions.insert("lodash", "^6.7")?;
        resolutions.insert("preact@^1", "^10")?;
        resolutions.insert("kleur@^999999998/preact", "^2000")?;

        const IRRELEVANT_PARENT: [(&str, &str); 1] = [("irrelevant", "npm:nop@^1")];
        assert_eq!(
            resolutions.find(("nonexistent", "^5000"), || &IRRELEVANT_PARENT),
            None
        );
        assert_eq!(
            resolutions.find(("lodash", "^1337"), || &IRRELEVANT_PARENT),
            Some("^6.7")
        );
        assert_eq!(
            resolutions.find(("preact", "npm:^1"), || &IRRELEVANT_PARENT),
            Some("^10")
        );
        assert_eq!(
            resolutions.find(("preact", "^19"), || &[
                ("kleur", "^0"),
                ("kleur", "=999999998.1.0")
            ]),
            None
        );
        assert_eq!(
            resolutions.find(("preact", "^19"), || &[
                ("kleur", "^999999998"),
                ("kleur", "=999999998.1.0")
            ]),
            Some("^2000")
        );

        Ok(())
    }
}
