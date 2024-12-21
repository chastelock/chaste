// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::Path;

#[cfg(feature = "npm")]
pub use chaste_npm as npm;
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
    let npm_lock = root_path.join(npm::LOCKFILE_NAME);
    if npm_lock.exists() {
        return Ok(npm::parse(root_path)?);
    }

    let yarn_lock = root_path.join(yarn::LOCKFILE_NAME);
    if yarn_lock.exists() {
        return Ok(yarn::parse(root_path)?);
    }

    Err(Error::NoLockfile)
}
