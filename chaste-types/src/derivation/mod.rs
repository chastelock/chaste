// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use crate::error::Result;
use crate::package::PackageID;

mod patch;

pub use patch::{PackagePatch, PackagePatchBuilder};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackageDerivationMeta {
    derivation: PackageDerivation,
    from: PackageID,
}

impl PackageDerivationMeta {
    pub fn derivation(&self) -> &PackageDerivation {
        &self.derivation
    }

    pub fn derived_from(&self) -> PackageID {
        self.from
    }

    pub fn patch(&self) -> Option<&patch::PackagePatch> {
        match &self.derivation {
            PackageDerivation::Patch(package_patch) => Some(package_patch),
            // _ => None,
        }
    }
}

pub struct PackageDerivationMetaBuilder {
    derivation: PackageDerivation,
    from: PackageID,
}

impl PackageDerivationMetaBuilder {
    pub fn new(derivation: PackageDerivation, from: PackageID) -> Self {
        Self { derivation, from }
    }

    pub fn build(self) -> Result<PackageDerivationMeta> {
        Ok(PackageDerivationMeta {
            derivation: self.derivation,
            from: self.from,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PackageDerivation {
    Patch(PackagePatch),
}
