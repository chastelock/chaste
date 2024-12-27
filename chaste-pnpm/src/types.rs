// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageJson<'a> {
    pub(crate) name: Option<&'a str>,
    pub(crate) version: Option<&'a str>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Lockfile<'a> {
    pub(crate) lockfile_version: &'a str,
    pub(crate) importers: HashMap<&'a str, lock::Importer<'a>>,
    #[serde(default)]
    pub(crate) packages: HashMap<&'a str, lock::Package<'a>>,
    #[serde(default)]
    pub(crate) snapshots: HashMap<&'a str, lock::Snapshot<'a>>,
}

pub(crate) mod lock {
    use std::collections::HashMap;

    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Importer<'a> {
        #[serde(borrow, default)]
        pub(crate) dependencies: HashMap<&'a str, ImporterDependency<'a>>,
        #[serde(borrow, default)]
        pub(crate) dev_dependencies: HashMap<&'a str, ImporterDependency<'a>>,
        // TODO: make sure it's all types
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct ImporterDependency<'a> {
        pub(crate) specifier: &'a str,
        pub(crate) version: &'a str,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Package<'a> {
        #[serde(borrow)]
        pub(crate) resolution: Resolution<'a>,
        pub(crate) version: Option<&'a str>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Resolution<'a> {
        pub(crate) integrity: Option<&'a str>,
        pub(crate) tarball: Option<&'a str>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub(crate) struct Snapshot<'a> {
        #[serde(borrow, default)]
        pub(crate) dependencies: HashMap<&'a str, &'a str>,
    }
}
