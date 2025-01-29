// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageRelations<'a> {
    #[serde(default)]
    pub dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub dev_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub peer_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub optional_dependencies: HashMap<Cow<'a, str>, Cow<'a, str>>,
    #[serde(default)]
    pub optional_peers: HashSet<Cow<'a, str>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMember<'a> {
    pub name: Option<Cow<'a, str>>,
    pub version: Option<Cow<'a, str>>,
    #[serde(flatten)]
    pub relations: PackageRelations<'a>,
}

#[derive(Debug)]
pub enum LockPackage<'a> {
    Registry {
        descriptor: Cow<'a, str>,
        registry_url: Cow<'a, str>,
        relations: PackageRelations<'a>,
        integrity: Cow<'a, str>,
    },
    Tarball {
        descriptor: Cow<'a, str>,
        relations: PackageRelations<'a>,
    },
    Git {
        descriptor: Cow<'a, str>,
        relations: PackageRelations<'a>,
        hash: Cow<'a, str>,
    },
    WorkspaceMember {
        descriptor: Cow<'a, str>,
    },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum LockPackageIR<'a> {
    Registry(
        Cow<'a, str>,
        Cow<'a, str>,
        PackageRelations<'a>,
        Cow<'a, str>,
    ),
    Tarball(Cow<'a, str>, PackageRelations<'a>),
    Git(Cow<'a, str>, PackageRelations<'a>, Cow<'a, str>),
    WorkspaceMember((Cow<'a, str>,)),
}

impl<'de, 'a> Deserialize<'de> for LockPackage<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
        Self: 'a,
    {
        LockPackageIR::deserialize(deserializer).map(|ir| match ir {
            LockPackageIR::Registry(descriptor, registry_url, relations, integrity) => {
                LockPackage::Registry {
                    descriptor,
                    registry_url,
                    relations,
                    integrity,
                }
            }
            LockPackageIR::Tarball(descriptor, relations) => LockPackage::Tarball {
                descriptor,
                relations,
            },
            LockPackageIR::Git(descriptor, relations, hash) => LockPackage::Git {
                descriptor,
                relations,
                hash,
            },
            LockPackageIR::WorkspaceMember((descriptor,)) => {
                LockPackage::WorkspaceMember { descriptor }
            }
        })
    }
}

impl<'a> LockPackage<'a> {
    pub fn descriptor(&self) -> &str {
        match self {
            LockPackage::Registry { descriptor, .. } => &descriptor,
            LockPackage::Tarball { descriptor, .. } => &descriptor,
            LockPackage::Git { descriptor, .. } => &descriptor,
            LockPackage::WorkspaceMember { descriptor } => &descriptor,
        }
    }
    pub fn relations(&self) -> Option<&PackageRelations> {
        match self {
            LockPackage::Registry { relations, .. } => Some(relations),
            LockPackage::Tarball { relations, .. } => Some(relations),
            LockPackage::Git { relations, .. } => Some(relations),
            LockPackage::WorkspaceMember { .. } => None,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BunLock<'a> {
    pub lockfile_version: u8,
    pub workspaces: HashMap<Cow<'a, str>, WorkspaceMember<'a>>,
    pub packages: HashMap<Cow<'a, str>, LockPackage<'a>>,
}
