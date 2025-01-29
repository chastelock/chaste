// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("No lockfile was found in the root directory")]
    NoLockfile,

    #[error("I/O error: {0:?}")]
    IoError(#[from] std::io::Error),

    #[error("Chaste core error: {0:?}")]
    CoreError(#[from] chaste_types::Error),

    #[error("Chaste bun error: {0:?}")]
    #[cfg(feature = "bun")]
    BunError(#[from] chaste_bun::Error),

    #[error("Chaste npm error: {0:?}")]
    #[cfg(feature = "npm")]
    NpmError(#[from] chaste_npm::Error),

    #[error("Chaste pnpm error: {0:?}")]
    #[cfg(feature = "pnpm")]
    PnpmError(#[from] chaste_pnpm::Error),

    #[error("Chaste yarn error: {0:?}")]
    #[cfg(any(feature = "yarn-berry", feature = "yarn-classic"))]
    YarnError(#[from] chaste_yarn::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
