// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use crate::package::PackageID;
use crate::svd::SourceVersionDescriptor;

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
    pub fn is_prod(self) -> bool {
        matches!(self, DependencyKind::DevDependency)
    }
    pub fn is_dev(self) -> bool {
        matches!(self, DependencyKind::DevDependency)
    }
    pub fn is_optional(self) -> bool {
        matches!(
            self,
            DependencyKind::OptionalDependency | DependencyKind::OptionalPeerDependency
        )
    }
}

#[derive(Debug)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub from: PackageID,
    pub on: PackageID,
    svd: Option<SourceVersionDescriptor>,
}

impl Dependency {
    pub fn svd(&self) -> Option<&SourceVersionDescriptor> {
        self.svd.as_ref()
    }
}

pub struct DependencyBuilder {
    kind: DependencyKind,
    of: PackageID,
    on: PackageID,
    svd: Option<SourceVersionDescriptor>,
}

impl DependencyBuilder {
    pub fn new(kind: DependencyKind, of: PackageID, on: PackageID) -> DependencyBuilder {
        DependencyBuilder {
            kind,
            of,
            on,
            svd: None,
        }
    }

    pub fn svd(&mut self, svd: SourceVersionDescriptor) {
        self.svd = Some(svd);
    }

    pub fn build(self) -> Dependency {
        Dependency {
            kind: self.kind,
            from: self.of,
            on: self.on,
            svd: self.svd,
        }
    }
}
