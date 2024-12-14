// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

pub use nodejs_semver::{SemverError, SemverErrorKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error("Root package id was not set when building a Chastefile")]
    MissingRootPackageID,

    #[error("Invalid package name: {0:?}")]
    InvalidPackageName(String),

    #[error("Semver error: {0:?}")]
    SemverError(#[from] SemverError),

    #[error("Invalid source/version descriptor: {0:?}")]
    SVDError(#[from] SVDError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error, PartialEq)]
pub enum SVDError {
    #[error("Unrecognized source/version descriptor type: {0:?}")]
    UnrecognizedType(String),
}
