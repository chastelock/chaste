// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use crate::error::Result;
use crate::module_path::ModulePath;
use crate::package::PackageID;

#[derive(Debug, Clone)]
pub struct Installation {
    package_id: PackageID,
    path: ModulePath,
}

impl Installation {
    pub fn package_id(&self) -> PackageID {
        self.package_id
    }

    pub fn path(&self) -> &ModulePath {
        &self.path
    }
}

#[derive(Debug)]
pub struct InstallationBuilder {
    package_id: PackageID,
    path: ModulePath,
}

impl InstallationBuilder {
    pub fn new(package_id: PackageID, path: ModulePath) -> Self {
        Self { package_id, path }
    }

    pub fn build(self) -> Result<Installation> {
        Ok(Installation {
            package_id: self.package_id,
            path: self.path,
        })
    }
}
