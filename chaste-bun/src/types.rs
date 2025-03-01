// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "fuzzing", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct PackageRelations<'a> {
    #[serde(default)]
    pub dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub dev_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub peer_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub optional_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub optional_peers: HashSet<Cow<'a, str>>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "fuzzing", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMember<'a> {
    pub name: Option<Cow<'a, str>>,
    pub version: Option<Cow<'a, str>>,
    #[serde(flatten)]
    pub relations: Box<PackageRelations<'a>>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "fuzzing", derive(arbitrary::Arbitrary))]
#[serde(untagged)]
pub enum LockPackageElement<'a> {
    Relations(Box<PackageRelations<'a>>),
    String(Cow<'a, str>),
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "fuzzing", derive(arbitrary::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct BunLock<'a> {
    pub lockfile_version: u8,
    pub workspaces: HashMap<Cow<'a, str>, WorkspaceMember<'a>>,
    pub packages: HashMap<Cow<'a, str>, Vec<LockPackageElement<'a>>>,
}
