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
        match self {
            DependencyKind::DevDependency => false,
            _ => true,
        }
    }
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
