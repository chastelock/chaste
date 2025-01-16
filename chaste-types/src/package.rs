// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::cmp;

pub use nodejs_semver::Version as PackageVersion;

use crate::checksums::Checksums;
use crate::error::Result;
use crate::name::PackageName;
use crate::source::{PackageSource, PackageSourceType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    name: Option<PackageName>,
    version: Option<PackageVersion>,
    checksums: Option<Checksums>,
    source: Option<PackageSource>,
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
}

impl PackageBuilder {
    pub fn new(name: Option<PackageName>, version: Option<String>) -> Self {
        PackageBuilder {
            name,
            version,
            checksums: None,
            source: None,
        }
    }

    pub fn get_name(&self) -> Option<&PackageName> {
        self.name.as_ref()
    }

    pub fn name(&mut self, new_name: Option<PackageName>) {
        self.name = new_name;
    }

    pub fn checksums(&mut self, new_checksums: Checksums) {
        self.checksums = Some(new_checksums);
    }

    pub fn source(&mut self, new_source: PackageSource) {
        self.source = Some(new_source);
    }

    pub fn build(self) -> Result<Package> {
        Ok(Package {
            name: self.name,
            version: self.version.map(PackageVersion::parse).transpose()?,
            checksums: self.checksums,
            source: self.source,
        })
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct PackageID(pub(crate) u64);
