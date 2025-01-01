// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use crate::package::PackageID;
use crate::svs::SourceVersionSpecifier;

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

#[derive(Debug, Clone)]
pub struct Dependency {
    pub kind: DependencyKind,
    pub from: PackageID,
    pub on: PackageID,
    svs: Option<SourceVersionSpecifier>,
}

impl Dependency {
    pub fn svs(&self) -> Option<&SourceVersionSpecifier> {
        self.svs.as_ref()
    }
}

pub struct DependencyBuilder {
    kind: DependencyKind,
    of: PackageID,
    on: PackageID,
    svs: Option<SourceVersionSpecifier>,
}

impl DependencyBuilder {
    pub fn new(kind: DependencyKind, of: PackageID, on: PackageID) -> DependencyBuilder {
        DependencyBuilder {
            kind,
            of,
            on,
            svs: None,
        }
    }

    pub fn svs(&mut self, svs: SourceVersionSpecifier) {
        self.svs = Some(svs);
    }

    pub fn build(self) -> Dependency {
        Dependency {
            kind: self.kind,
            from: self.of,
            on: self.on,
            svs: self.svs,
        }
    }
}
