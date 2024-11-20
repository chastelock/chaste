// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use crate::package::PackageID;

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
}
