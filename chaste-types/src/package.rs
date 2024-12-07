// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

pub use nodejs_semver::Version as PackageVersion;
pub use ssri::{Error as SSRIError, Integrity};

use crate::error::Result;
use crate::name::PackageName;

#[derive(Debug)]
pub struct Package {
    name: Option<PackageName>,
    version: Option<PackageVersion>,
    integrity: Integrity,
}

impl Package {
    pub fn name(&self) -> Option<&PackageName> {
        self.name.as_ref()
    }

    pub fn version(&self) -> Option<&PackageVersion> {
        self.version.as_ref()
    }

    pub fn integrity(&self) -> &Integrity {
        &self.integrity
    }
}

pub struct PackageBuilder {
    name: Option<PackageName>,
    version: Option<String>,
    integrity: Option<Integrity>,
}

impl PackageBuilder {
    pub fn new(name: Option<PackageName>, version: Option<String>) -> Self {
        PackageBuilder {
            name,
            version,
            integrity: None,
        }
    }

    pub fn get_name(&self) -> Option<&PackageName> {
        self.name.as_ref()
    }

    pub fn name(&mut self, new_name: Option<PackageName>) {
        self.name = new_name;
    }

    pub fn integrity(&mut self, new_integrity: Integrity) {
        self.integrity = Some(new_integrity);
    }

    pub fn build(self) -> Result<Package> {
        Ok(Package {
            name: self.name,
            version: self.version.map(PackageVersion::parse).transpose()?,
            integrity: self.integrity.unwrap_or_else(|| Integrity::from("")),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct PackageID(pub(crate) u64);
