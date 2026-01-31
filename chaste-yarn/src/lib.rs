// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::{Path, PathBuf};
use std::{fs, io, str};

use chaste_types::{Chastefile, ProviderMeta};
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

#[derive(Debug, Clone)]
pub struct Meta {
    pub lockfile_version: u8,
}

impl ProviderMeta for Meta {
    fn provider_name(&self) -> &'static str {
        "yarn"
    }
}

pub fn parse<P>(root_dir: P) -> Result<Chastefile<Meta>>
where
    P: AsRef<Path>,
{
    let root_dir = root_dir.as_ref();

    let lockfile_contents = fs::read_to_string(root_dir.join(LOCKFILE_NAME))?;
    parse_real(&lockfile_contents, root_dir, &fs::read_to_string)
}

fn parse_real<FG>(
    lockfile_contents: &str,
    root_dir: &Path,
    file_getter: &FG,
) -> Result<Chastefile<Meta>>
where
    FG: Fn(PathBuf) -> Result<String, io::Error>,
{
    let yarn_lock: yarn::Lockfile = yarn::parse_str(&lockfile_contents)?;
    match yarn_lock.version {
        #[cfg(feature = "classic")]
        1 => classic::resolve(yarn_lock, root_dir, file_getter),
        #[cfg(feature = "berry")]
        2..=8 => berry::resolve(yarn_lock, root_dir, file_getter),
        _ => Err(Error::UnknownLockfileVersion(yarn_lock.version)),
    }
}

#[cfg(feature = "fuzzing")]
pub fn parse_arbitrary<FG>(
    lockfile_contents: &str,
    root_dir: &Path,
    file_getter: &FG,
) -> Result<Chastefile<Meta>>
where
    FG: Fn(PathBuf) -> Result<String, io::Error>,
{
    parse_real(lockfile_contents, root_dir, file_getter)
}
