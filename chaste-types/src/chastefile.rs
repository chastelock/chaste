// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::collections::{HashMap, HashSet, VecDeque};

use crate::dependency::Dependency;
use crate::error::{Error, Result};
use crate::installation::Installation;
use crate::package::{Package, PackageID};

#[derive(Debug, Clone)]
pub struct Chastefile {
    packages: HashMap<PackageID, Package>,
    installations: Vec<Installation>,
    dependencies: Vec<Dependency>,
    root_package_id: PackageID,
    workspace_members: Vec<PackageID>,
}

impl<'a> Chastefile {
    pub fn package(&'a self, package_id: PackageID) -> &'a Package {
        self.packages.get(&package_id).unwrap()
    }

    pub fn packages(&'a self) -> Vec<&'a Package> {
        self.packages.values().collect()
    }

    pub fn packages_with_ids(&'a self) -> Vec<(PackageID, &'a Package)> {
        self.packages.iter().map(|(pid, pkg)| (*pid, pkg)).collect()
    }

    fn package_dependencies_iter(
        &'a self,
        package_id: PackageID,
    ) -> impl Iterator<Item = &'a Dependency> {
        self.dependencies
            .iter()
            .filter(move |d| d.from == package_id)
    }

    fn package_prod_dependencies_iter(
        &'a self,
        package_id: PackageID,
    ) -> impl Iterator<Item = &'a Dependency> {
        self.dependencies
            .iter()
            .filter(move |d| d.kind.is_prod() && d.from == package_id)
    }

    /// Direct dependencies of any kind from specified package
    pub fn package_dependencies(&'a self, package_id: PackageID) -> Vec<&'a Dependency> {
        self.package_dependencies_iter(package_id).collect()
    }

    /// Direct dependencies of any kind other than [`crate::DependencyKind::DevDependency`] from specified package
    pub fn package_prod_dependencies(&'a self, package_id: PackageID) -> Vec<&'a Dependency> {
        self.package_prod_dependencies_iter(package_id).collect()
    }

    /// Dependencies, direct and transitive, of any kind from specified package
    pub fn recursive_package_dependencies(&'a self, package_id: PackageID) -> Vec<&'a Dependency> {
        let mut result = self.package_dependencies(package_id);
        let mut seen = HashSet::with_capacity(result.len());
        let mut q = VecDeque::with_capacity(result.len());
        result.iter().for_each(|d| {
            seen.insert(d.on);
            q.push_back(d.on);
        });
        while let Some(pid) = q.pop_front() {
            for dep in self.package_dependencies_iter(pid) {
                if seen.insert(dep.on) {
                    q.push_back(dep.on);
                    result.push(dep);
                }
            }
        }
        result
    }

    /// Dependencies, direct and transitive, of any kind other than [`crate::DependencyKind::DevDependency`]
    /// from specified package
    pub fn recursive_prod_package_dependencies(
        &'a self,
        package_id: PackageID,
    ) -> Vec<&'a Dependency> {
        let mut result = self.package_prod_dependencies(package_id);
        let mut seen = HashSet::with_capacity(result.len());
        let mut q = VecDeque::with_capacity(result.len());
        result.iter().for_each(|d| {
            seen.insert(d.on);
            q.push_back(d.on);
        });
        while let Some(pid) = q.pop_front() {
            for dep in self.package_prod_dependencies_iter(pid) {
                if seen.insert(dep.on) {
                    q.push_back(dep.on);
                    result.push(dep);
                }
            }
        }
        result
    }

    fn package_dependents_iter(
        &'a self,
        package_id: PackageID,
    ) -> impl Iterator<Item = &'a Dependency> {
        self.dependencies.iter().filter(move |d| d.on == package_id)
    }

    /// Direct dependencies of any kind *on* the specified package (reverse dependencies)
    pub fn package_dependents(&'a self, package_id: PackageID) -> Vec<&'a Dependency> {
        self.package_dependents_iter(package_id).collect()
    }

    pub fn root_package_id(&'a self) -> PackageID {
        self.root_package_id
    }

    pub fn root_package(&'a self) -> &'a Package {
        self.packages.get(&self.root_package_id).unwrap()
    }

    pub fn root_package_dependencies(&'a self) -> Vec<&'a Dependency> {
        self.package_dependencies(self.root_package_id)
    }

    pub fn root_package_prod_dependencies(&'a self) -> Vec<&'a Dependency> {
        self.package_prod_dependencies(self.root_package_id)
    }

    pub fn workspace_member_ids(&'a self) -> &'a [PackageID] {
        &self.workspace_members
    }

    pub fn workspace_members(&'a self) -> Vec<&'a Package> {
        self.workspace_members
            .iter()
            .map(|pid| self.package(*pid))
            .collect()
    }

    pub fn package_installations(&'a self, package_id: PackageID) -> Vec<&'a Installation> {
        self.installations
            .iter()
            .filter(|i| i.package_id() == package_id)
            .collect()
    }
}

#[derive(Debug)]
pub struct ChastefileBuilder {
    packages: HashMap<PackageID, Package>,
    dependencies: Vec<Dependency>,
    installations: Vec<Installation>,
    next_pid: u64,
    root_package_id: Option<PackageID>,
    workspace_members: Vec<PackageID>,
}

impl ChastefileBuilder {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            packages: HashMap::new(),
            dependencies: Vec::new(),
            installations: Vec::new(),
            next_pid: 0,
            root_package_id: None,
            workspace_members: Vec::new(),
        }
    }

    fn new_pid(&mut self) -> PackageID {
        let pid = PackageID(self.next_pid);
        self.next_pid += 1;
        pid
    }

    pub fn add_package(&mut self, package: Package) -> Result<PackageID> {
        if let Some((original_pid, _)) = self.packages.iter().find(|(_, p)| *p == &package) {
            return Err(Error::DuplicatePackage(*original_pid));
        }
        let pid = self.new_pid();
        self.packages.insert(pid, package);
        Ok(pid)
    }

    pub fn add_package_installation(&mut self, installation: Installation) {
        self.installations.push(installation);
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

    pub fn set_as_workspace_member(&mut self, member_pid: PackageID) -> Result<()> {
        self.workspace_members.push(member_pid);
        Ok(())
    }

    pub fn build(self) -> Result<Chastefile> {
        Ok(Chastefile {
            packages: self.packages,
            dependencies: self.dependencies,
            installations: self.installations,
            root_package_id: self.root_package_id.ok_or(Error::MissingRootPackageID)?,
            workspace_members: self.workspace_members,
        })
    }
}
