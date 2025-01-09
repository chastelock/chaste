// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::Path;
use std::{fs, str};

use chaste_types::Chastefile;
use yarn_lock_parser as yarn;

pub use crate::error::{Error, Result};

#[cfg(feature = "berry")]
mod berry;
#[cfg(feature = "classic")]
mod classic;
mod error;
#[cfg(test)]
mod tests;

pub static LOCKFILE_NAME: &str = "yarn.lock";

pub fn parse<P>(root_dir: P) -> Result<Chastefile>
where
    P: AsRef<Path>,
{
    let root_dir = root_dir.as_ref();

    let lockfile_contents = fs::read_to_string(root_dir.join(LOCKFILE_NAME))?;
    let yarn_lock: yarn::Lockfile = yarn::parse_str(&lockfile_contents)?;

    match yarn_lock.version {
        #[cfg(feature = "classic")]
        1 => classic::resolve(yarn_lock, root_dir),
        #[cfg(feature = "berry")]
        2..=8 => berry::resolve(yarn_lock, root_dir),
        _ => Err(Error::UnknownLockfileVersion(yarn_lock.version)),
    }
}
