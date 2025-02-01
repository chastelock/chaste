// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageJson<'a> {
    pub(crate) name: Option<Cow<'a, str>>,
    pub(crate) version: Option<Cow<'a, str>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Lockfile<'a> {
    pub(crate) lockfile_version: &'a str,
    pub(crate) importers: HashMap<&'a str, lock::Importer<'a>>,
    #[serde(default)]
    pub(crate) packages: HashMap<Cow<'a, str>, lock::Package<'a>>,
    #[serde(default)]
    pub(crate) snapshots: HashMap<Cow<'a, str>, lock::Snapshot<'a>>,
}

pub(crate) mod lock {
    use std::borrow::Cow;
    use std::collections::HashMap;

    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Importer<'a> {
        #[serde(borrow, default)]
        pub(crate) dependencies: HashMap<Cow<'a, str>, ImporterDependency<'a>>,
        #[serde(borrow, default)]
        pub(crate) dev_dependencies: HashMap<Cow<'a, str>, ImporterDependency<'a>>,
        // TODO: make sure it's all types
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ImporterDependency<'a> {
        pub(crate) specifier: Cow<'a, str>,
        pub(crate) version: Cow<'a, str>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Package<'a> {
        #[serde(borrow)]
        pub(crate) resolution: Resolution<'a>,
        pub(crate) version: Option<Cow<'a, str>>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Resolution<'a> {
        pub(crate) integrity: Option<&'a str>,
        pub(crate) tarball: Option<Cow<'a, str>>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Snapshot<'a> {
        #[serde(borrow, default)]
        pub(crate) dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    }
}
