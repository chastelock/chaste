// SPDX-FileCopyrightText: 2026 The Chaste Authors
// SPDX-License-Identifier: BSD-2-Clause

use std::borrow::Cow;
use std::collections::BTreeMap;

#[derive(Debug, serde::Deserialize, yoke::Yokeable)]
#[serde(rename_all = "camelCase")]
pub struct PackageJson<'y> {
    #[serde(borrow)]
    pub name: Option<Cow<'y, str>>,
    #[serde(borrow)]
    pub version: Option<Cow<'y, str>>,
    #[serde(default)]
    pub(crate) workspaces: Option<Vec<Cow<'y, str>>>,
    #[serde(default, borrow)]
    pub dependencies: BTreeMap<Cow<'y, str>, Cow<'y, str>>,
    #[serde(default, borrow)]
    pub dev_dependencies: BTreeMap<Cow<'y, str>, Cow<'y, str>>,
    #[serde(default, borrow)]
    pub optional_dependencies: BTreeMap<Cow<'y, str>, Cow<'y, str>>,
    #[serde(default, borrow)]
    pub peer_dependencies: BTreeMap<Cow<'y, str>, Cow<'y, str>>,
    #[serde(default, borrow)]
    pub resolutions: BTreeMap<Cow<'y, str>, Cow<'y, str>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lockfile<'y> {
    #[serde(rename = "__metadata")]
    pub metadata: Metadata,
    #[serde(borrow)]
    pub entries: BTreeMap<&'y str, Entry<'y>>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub version: u8,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry<'y> {
    pub checksum: Option<&'y str>,
    pub resolution: Resolution<'y>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resolution<'y> {
    pub resolution: &'y str,
    pub version: &'y str,
    #[serde(default)]
    pub dependencies: BTreeMap<&'y str, &'y str>,
    #[serde(default)]
    pub peer_dependencies: BTreeMap<&'y str, &'y str>,
    #[serde(default)]
    pub optional_dependencies: Vec<&'y str>,
    #[serde(default)]
    pub optional_peer_dependencies: Vec<&'y str>,
}
