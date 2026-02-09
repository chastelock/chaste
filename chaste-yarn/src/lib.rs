// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::path::{Path, PathBuf};
use std::{fs, io, str};

use chaste_types::{Chastefile, LockfileVersion, ProviderMeta};
use nom::branch::alt;
use nom::bytes::streaming::tag;
use nom::character::complete::space0;
use nom::sequence::preceded;
use nom::Parser;
#[cfg(any(feature = "classic", feature = "berry"))]
use yarn_lock_parser as yarn;

pub use crate::error::{Error, Result};

#[cfg(feature = "berry")]
mod berry;
#[cfg(any(feature = "berry", feature = "zpm"))]
pub(crate) mod btree_candidates;
#[cfg(feature = "classic")]
mod classic;
mod error;
#[cfg(any(feature = "berry", feature = "zpm"))]
pub(crate) mod resolutions;
#[cfg(test)]
mod tests;
#[cfg(feature = "zpm")]
mod zpm;

pub static LOCKFILE_NAME: &str = "yarn.lock";

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Meta {
    pub lockfile_version: u8,
}

impl ProviderMeta for Meta {
    fn provider_name(&self) -> &'static str {
        "yarn"
    }

    fn lockfile_version<'m>(&'m self) -> Option<LockfileVersion<'m>> {
        Some(LockfileVersion::U8(self.lockfile_version))
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
    enum Format {
        Indented,
        Json,
    }
    match preceded(
        space0::<&str, ()>,
        alt((
            tag("{").map(|_| Format::Json),
            tag("#").map(|_| Format::Indented),
        )),
    )
    .parse(lockfile_contents)
    {
        #[cfg(any(feature = "classic", feature = "berry"))]
        Ok((_, Format::Indented)) => {
            let yarn_lock: yarn::Lockfile = yarn::parse_str(&lockfile_contents)?;
            match yarn_lock.version {
                #[cfg(feature = "classic")]
                1 => classic::resolve(yarn_lock, root_dir, file_getter),
                #[cfg(feature = "berry")]
                2..=8 => berry::resolve(yarn_lock, root_dir, file_getter),
                _ => Err(Error::UnknownLockfileVersion(yarn_lock.version)),
            }
        }
        #[cfg(not(any(feature = "classic", feature = "berry")))]
        Ok((_, Format::Indented)) => Err(Error::UnknownFormat),
        #[cfg(feature = "zpm")]
        Ok((_, Format::Json)) => zpm::resolve(lockfile_contents, root_dir, file_getter),
        #[cfg(not(feature = "zpm"))]
        Ok((_, Format::Json)) => Err(Error::UnknownFormat),

        Err(_) => Err(Error::UnknownFormat),
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
