// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::cmp;

pub use nodejs_semver::Version as PackageVersion;

use crate::checksums::Checksums;
use crate::derivation::{PackageDerivation, PackageDerivationMeta};
use crate::error::Result;
use crate::name::PackageName;
use crate::source::{PackageSource, PackageSourceType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    name: Option<PackageName>,
    version: Option<PackageVersion>,
    checksums: Option<Checksums>,
    source: Option<PackageSource>,
    derived: Option<PackageDerivationMeta>,
}

impl Package {
    pub fn name(&self) -> Option<&PackageName> {
        self.name.as_ref()
    }

    pub fn version(&self) -> Option<&PackageVersion> {
        self.version.as_ref()
    }

    pub fn checksums(&self) -> Option<&Checksums> {
        self.checksums.as_ref()
    }

    pub fn source(&self) -> Option<&PackageSource> {
        self.source.as_ref()
    }

    pub fn source_type(&self) -> Option<PackageSourceType> {
        self.source.as_ref().map(|s| s.source_type())
    }

    pub fn derivation(&self) -> Option<&PackageDerivation> {
        self.derived.as_ref().map(|d| d.derivation())
    }

    pub fn derivation_meta(&self) -> Option<&PackageDerivationMeta> {
        self.derived.as_ref()
    }

    pub fn derived_from(&self) -> Option<PackageID> {
        self.derived.as_ref().map(|d| d.derived_from())
    }

    pub fn is_derived(&self) -> bool {
        self.derived.is_some()
    }

    pub(crate) fn is_duplicate_of(&self, other: &Package) -> bool {
        self.eq(other) && self.source.is_some()
    }
}

impl PartialOrd for Package {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        let o = self.name.cmp(&other.name);
        if o != cmp::Ordering::Equal {
            return Some(o);
        }
        let o = self.version.cmp(&other.version);
        if o != cmp::Ordering::Equal {
            return Some(o);
        }
        None
    }
}

pub struct PackageBuilder {
    name: Option<PackageName>,
    version: Option<String>,
    checksums: Option<Checksums>,
    source: Option<PackageSource>,
    derived: Option<PackageDerivationMeta>,
}

impl PackageBuilder {
    pub fn new(name: Option<PackageName>, version: Option<String>) -> Self {
        PackageBuilder {
            name,
            version,
            checksums: None,
            source: None,
            derived: None,
        }
    }

    pub fn get_name(&self) -> Option<&PackageName> {
        self.name.as_ref()
    }

    pub fn name(&mut self, new_name: Option<PackageName>) {
        self.name = new_name;
    }

    pub fn version(&mut self, new_version: Option<String>) {
        self.version = new_version;
    }

    pub fn checksums(&mut self, new_checksums: Checksums) {
        self.checksums = Some(new_checksums);
    }

    pub fn source(&mut self, new_source: PackageSource) {
        self.source = Some(new_source);
    }

    pub fn derived(&mut self, new_derived: PackageDerivationMeta) {
        self.derived = Some(new_derived);
    }

    pub fn build(self) -> Result<Package> {
        Ok(Package {
            name: self.name,
            version: self.version.map(PackageVersion::parse).transpose()?,
            checksums: self.checksums.filter(|c| !c.integrity().hashes.is_empty()),
            source: self.source,
            derived: self.derived,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct PackageID(pub(crate) u64);
