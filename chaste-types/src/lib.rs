// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::HashMap;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[non_exhaustive]
pub enum DependencyKind {
    WorkspaceMember,
    Dependency,
    DevDependency,
    PeerDependency,
    OptionalDependency,
    BundleDependency,
}

#[derive(Debug)]
pub struct Package {
    pub name: Option<String>,
    pub version: Option<String>,
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
}

#[derive(Debug)]
pub struct ChastefileBuilder {
    packages: HashMap<PackageID, Package>,
    dependencies: Vec<Dependency>,
    next_pid: u64,
}

impl ChastefileBuilder {
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            dependencies: Vec::new(),
            next_pid: 0,
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

    pub fn add_dependencies(&mut self, dependencies: impl Iterator<Item = Dependency>) {
        self.dependencies.extend(dependencies);
    }

    pub fn build(self) -> Chastefile {
        Chastefile {
            packages: self.packages,
            dependencies: self.dependencies,
        }
    }
}
