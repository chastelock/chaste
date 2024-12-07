// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

pub use nodejs_semver::{SemverError, SemverErrorKind};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
#[non_exhaustive]
pub enum Error {
    #[error("Root package id was not set when building a Chastefile")]
    MissingRootPackageID,

    #[error("Semver error: {0:?}")]
    SemverError(#[from] SemverError),

    #[error("Invalid package name: {0:?}")]
    PackageNameError(#[from] PackageNameError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error, PartialEq)]
#[cfg_attr(feature = "miette", derive(miette::Diagnostic))]
#[non_exhaustive]
pub enum PackageNameError {
    #[error("Invalid character: {char:?}")]
    InvalidCharacter {
        char: char,
        #[cfg(feature = "miette")]
        at: miette::SourceSpan,
    },

    #[error("Unexpected end")]
    UnexpectedEnd {
        #[cfg(feature = "miette")]
        at: miette::SourceSpan,
    },
}
