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

pub fn from_root_path<P>(root_path: P) -> Result<Chastefile>
where
    P: AsRef<Path>,
{
    let root_path = root_path.as_ref();

    #[cfg(feature = "bun")]
    {
        if root_path.join(bun::LOCKFILE_NAME).exists() {
            return Ok(bun::parse(root_path)?);
        }
    }

    #[cfg(feature = "npm")]
    {
        if root_path.join(npm::SHRINKWRAP_NAME).exists()
            || root_path.join(npm::LOCKFILE_NAME).exists()
        {
            return Ok(npm::parse(root_path)?);
        }
    }

    #[cfg(feature = "pnpm")]
    {
        if root_path.join(pnpm::LOCKFILE_NAME).exists() {
            return Ok(pnpm::parse(root_path)?);
        }
    }

    #[cfg(any(feature = "yarn-berry", feature = "yarn-classic"))]
    {
        if root_path.join(yarn::LOCKFILE_NAME).exists() {
            return Ok(yarn::parse(root_path)?);
        }
    }

    Err(Error::NoLockfile)
}
