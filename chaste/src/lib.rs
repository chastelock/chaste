// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::Path;

#[cfg(feature = "bun")]
pub use chaste_bun as bun;
#[cfg(feature = "npm")]
pub use chaste_npm as npm;
#[cfg(feature = "pnpm")]
pub use chaste_pnpm as pnpm;
pub use chaste_types as types;
#[cfg(any(feature = "yarn-berry", feature = "yarn-classic"))]
pub use chaste_yarn as yarn;

pub use chaste_types::{Chastefile, Dependency, DependencyKind, Package, PackageID};

pub mod error;
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub enum Meta {
    #[cfg(feature = "bun")]
    Bun(bun::Meta),

    #[cfg(feature = "npm")]
    Npm(npm::Meta),

    #[cfg(feature = "pnpm")]
    Pnpm(pnpm::Meta),

    #[cfg(any(feature = "yarn-classic", feature = "yarn-berry"))]
    Yarn(yarn::Meta),
}

impl Meta {
    fn on_inner<'m, F, O>(&'m self, func: F) -> O
    where
        F: FnOnce(&'m dyn types::ProviderMeta) -> O,
    {
        match self {
            #[cfg(feature = "bun")]
            Meta::Bun(meta) => func(meta),
            #[cfg(feature = "npm")]
            Meta::Npm(meta) => func(meta),
            #[cfg(feature = "pnpm")]
            Meta::Pnpm(meta) => func(meta),
            #[cfg(any(feature = "yarn-classic", feature = "yarn-berry"))]
            Meta::Yarn(meta) => func(meta),
            #[cfg(not(any(
                feature = "bun",
                feature = "npm",
                feature = "pnpm",
                feature = "yarn-classic",
                feature = "yarn-berry"
            )))]
            _ => unreachable!(),
        }
    }
}

impl types::ProviderMeta for Meta {
    fn provider_name(&self) -> &'static str {
        self.on_inner(|m| m.provider_name())
    }

    fn lockfile_version<'m>(&'m self) -> Option<types::LockfileVersion<'m>> {
        self.on_inner(|m| m.lockfile_version())
    }
}

pub fn from_root_path<P>(root_path: P) -> Result<Chastefile<Meta>>
where
    P: AsRef<Path>,
{
    let root_path = root_path.as_ref();

    #[cfg(feature = "bun")]
    {
        if root_path.join(bun::LOCKFILE_NAME).exists() {
            return Ok(bun::parse(root_path)?.map_meta(Meta::Bun));
        }
    }

    #[cfg(feature = "npm")]
    {
        if root_path.join(npm::SHRINKWRAP_NAME).exists()
            || root_path.join(npm::LOCKFILE_NAME).exists()
        {
            return Ok(npm::parse(root_path)?.map_meta(Meta::Npm));
        }
    }

    #[cfg(feature = "pnpm")]
    {
        if root_path.join(pnpm::LOCKFILE_NAME).exists() {
            return Ok(pnpm::parse(root_path)?.map_meta(Meta::Pnpm));
        }
    }

    #[cfg(any(feature = "yarn-berry", feature = "yarn-classic"))]
    {
        if root_path.join(yarn::LOCKFILE_NAME).exists() {
            return Ok(yarn::parse(root_path)?.map_meta(Meta::Yarn));
        }
    }

    Err(Error::NoLockfile)
}
