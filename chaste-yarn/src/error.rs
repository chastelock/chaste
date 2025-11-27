// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::{io, path, str};

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Dependency {0:?} not found")]
    DependencyNotFound(String),

    #[error("Unknown yarn.lock version: {0:?}")]
    UnknownLockfileVersion(u8),

    #[error(".yarn-state.yml package {0:?} not found")]
    StatePackageNotFound(String),

    #[error("Invalid resolution key: {0:?}")]
    InvalidResolution(String),

    #[error("Couldn't recognize the resolution to follow to replace {0:?}")]
    AmbiguousResolution(String),

    #[error("Invalid resolved: {0:?}")]
    InvalidResolved(String),

    #[error("Ambiguous resolved: {0:?}")]
    AmbiguousResolved(String),

    #[error("Conflicting descriptors: {0:?} and {1:?}")]
    ConflictingDescriptors(String, String),

    #[error("Chaste error: {0:?}")]
    ChasteError(#[from] chaste_types::Error),

    #[error("I/O error: {0:?}")]
    IoError(#[from] io::Error),

    #[error("I/O error trying to read {1:?}: {0:?}")]
    IoInWorkspace(io::Error, path::PathBuf),

    #[error("UTF-8 parsing error: {0:?}")]
    Utf8Error(#[from] str::Utf8Error),

    #[error("Yarn parser error: {0:?}")]
    YarnParserError(#[from] yarn_lock_parser::YarnLockError),

    #[error("Glob error: {0:?}")]
    #[cfg(feature = "classic")]
    GlobreeksError(#[from] globreeks::Error),

    #[error("Walkdir error: {0:?}")]
    #[cfg(feature = "classic")]
    WalkdirError(#[from] walkdir::Error),

    #[error("Yarn state parser error: {0:?}")]
    #[cfg(feature = "berry")]
    YarnStateError(#[from] yarn_state::error::Error),

    #[error("SSRI error: {0:?}")]
    SSRIError(#[from] chaste_types::SSRIError),

    #[error("JSON parsing error: {0:?}")]
    SerdeJsonError(#[from] serde_json::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
