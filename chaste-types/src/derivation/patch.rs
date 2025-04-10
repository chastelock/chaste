// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use ssri::Integrity;

use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackagePatch {
    path: String,
    integrity: Option<Integrity>,
}

impl PackagePatch {
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Integrity of the patch file.
    pub fn integrity(&self) -> Option<&Integrity> {
        self.integrity.as_ref()
    }
}

pub struct PackagePatchBuilder {
    path: String,
    integrity: Option<Integrity>,
}

impl PackagePatchBuilder {
    pub fn new(path: String) -> Self {
        Self {
            path,
            integrity: None,
        }
    }

    pub fn integrity(&mut self, new_integrity: Integrity) {
        self.integrity = Some(new_integrity);
    }

    pub fn build(self) -> Result<PackagePatch> {
        Ok(PackagePatch {
            path: self.path,
            integrity: self.integrity,
        })
    }
}
