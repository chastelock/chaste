// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

pub use nodejs_semver::{SemverError, SemverErrorKind};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Root package id was not set when building a Chastefile")]
    MissingRootPackageID,

    #[error("Semver error: {0:?}")]
    SemverError(#[from] SemverError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
