// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

#[derive(Debug)]
pub struct Package {
    name: Option<String>,
    version: Option<String>,
    integrity: Option<String>,
    /// Complicated. Some lockfiles (npm) say it, but this depends on CLI and config options,
    /// as package managers implement multiple strategies.
    expected_path: Option<String>,
}

impl Package {
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn integrity(&self) -> Option<&str> {
        self.integrity.as_deref()
    }

    /// Complicated. Some lockfiles (npm) say it, but this depends on CLI and config options,
    /// as package managers implement multiple strategies.
    pub fn expected_path(&self) -> Option<&str> {
        self.expected_path.as_deref()
    }
}

pub struct PackageBuilder {
    name: Option<String>,
    version: Option<String>,
    integrity: Option<String>,
    expected_path: Option<String>,
}

impl PackageBuilder {
    pub fn new(name: Option<String>, version: Option<String>) -> Self {
        PackageBuilder {
            name,
            version,
            integrity: None,
            expected_path: None,
        }
    }

    pub fn get_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn name(&mut self, new_name: Option<String>) {
        self.name = new_name;
    }

    pub fn integrity(&mut self, new_integrity: Option<String>) {
        self.integrity = new_integrity;
    }

    pub fn expected_path(&mut self, new_path: Option<String>) {
        self.expected_path = new_path;
    }

    pub fn build(self) -> Package {
        Package {
            name: self.name,
            version: self.version,
            integrity: self.integrity,
            expected_path: self.expected_path,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub struct PackageID(pub(crate) u64);
