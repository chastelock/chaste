// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::borrow::Cow;
use std::collections::HashMap;

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackageJson<'a> {
    #[serde(default)]
    pub(crate) resolutions: HashMap<Cow<'a, str>, Cow<'a, str>>,
}
