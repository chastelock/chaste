// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageJson<'a> {
    pub(crate) name: Option<Cow<'a, str>>,
    pub(crate) version: Option<Cow<'a, str>>,
    pub(crate) workspaces: Option<Vec<Cow<'a, str>>>,
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
#[serde(rename_all = "camelCase")]
pub(crate) struct PeerDependencyMeta {
    pub(crate) optional: Option<bool>,
}
