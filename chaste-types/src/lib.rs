// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;

pub use crate::error::{Error, Result};

pub mod error;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum DependencyKind {
    Dependency,
    DevDependency,
    PeerDependency,
    OptionalDependency,
    OptionalPeerDependency,
}

impl DependencyKind {
    pub fn is_dev(self) -> bool {
        match self {
            DependencyKind::DevDependency => true,
            _ => false,
        }
    }
    pub fn is_optional(self) -> bool {
        match self {
            DependencyKind::OptionalDependency => true,
            DependencyKind::OptionalPeerDependency => true,
            _ => false,
        }
    }
}

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
pub struct PackageID(u64);

#[derive(Debug)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub from: PackageID,
    pub on: PackageID,
}

#[derive(Debug)]
pub struct Chastefile {
    packages: HashMap<PackageID, Package>,
    dependencies: Vec<Dependency>,
    root_package_id: PackageID,
}

impl<'a> Chastefile {
    pub fn packages(&'a self) -> Vec<&'a Package> {
        self.packages.values().collect()
    }

    pub fn packages_with_ids(&'a self) -> Vec<(PackageID, &'a Package)> {
        self.packages
            .iter()
            .map(|(pid, pkg)| (pid.clone(), pkg))
            .collect()
    }

    pub fn package_dependencies(&'a self, package_id: PackageID) -> Vec<&'a Dependency> {
        self.dependencies
            .iter()
            .filter(|d| d.from == package_id)
            .collect()
    }

    pub fn root_package_id(&'a self) -> PackageID {
        self.root_package_id
    }

    pub fn root_package(&'a self) -> &'a Package {
        self.packages.get(&self.root_package_id).unwrap()
    }
}

#[derive(Debug)]
pub struct ChastefileBuilder {
    packages: HashMap<PackageID, Package>,
    dependencies: Vec<Dependency>,
    next_pid: u64,
    root_package_id: Option<PackageID>,
}

impl ChastefileBuilder {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            dependencies: Vec::new(),
            next_pid: 0,
            root_package_id: None,
        }
    }

    fn new_pid(&mut self) -> PackageID {
        let pid = PackageID(self.next_pid);
        self.next_pid += 1;
        pid
    }

    pub fn add_package(&mut self, package: Package) -> PackageID {
        let pid = self.new_pid();
        self.packages.insert(pid, package);
        pid
    }

    pub fn add_dependency(&mut self, dependency: Dependency) {
        self.dependencies.push(dependency);
    }

    pub fn add_dependencies(&mut self, dependencies: impl Iterator<Item = Dependency>) {
        self.dependencies.extend(dependencies);
    }

    pub fn set_root_package_id(&mut self, root_pid: PackageID) -> Result<()> {
        self.root_package_id = Some(root_pid);
        Ok(())
    }

    pub fn build(self) -> Result<Chastefile> {
        Ok(Chastefile {
            packages: self.packages,
            dependencies: self.dependencies,
            root_package_id: self.root_package_id.ok_or(Error::MissingRootPackageID)?,
        })
    }
}
