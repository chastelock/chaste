// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "fuzzing", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub(crate) struct DependencyTreePackage<'a> {
    pub(crate) name: Option<Cow<'a, str>>,
    pub(crate) version: Option<Cow<'a, str>>,
    pub(crate) resolved: Option<Cow<'a, str>>,
    pub(crate) link: Option<bool>,
    pub(crate) integrity: Option<Cow<'a, str>>,
    #[serde(default)]
    pub(crate) dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub(crate) dev_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub(crate) peer_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub(crate) peer_dependencies_meta: HashMap<Cow<'a, str>, PeerDependencyMeta>,
    #[serde(default)]
    pub(crate) optional_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "fuzzing", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub(crate) struct PeerDependencyMeta {
    pub(crate) optional: Option<bool>,
}

#[derive(Deserialize, Debug)]
#[cfg_attr(feature = "fuzzing", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PackageLock<'a> {
    pub(crate) name: Cow<'a, str>,
    pub(crate) lockfile_version: u8,
    #[serde(default)]
    pub(crate) packages: HashMap<Cow<'a, str>, DependencyTreePackage<'a>>,
}
