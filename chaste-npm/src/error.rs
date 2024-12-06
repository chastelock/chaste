// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::io;

use thiserror::Error;

use crate::parsers::PathLexingError;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Dependency {0:?} not found")]
    DependencyNotFound(String),

    #[error("Unknown lockfile version: {0}")]
    UnknownLockVersion(u8),

    #[error("Chaste error: {0:?}")]
    ChasteError(#[from] chaste_types::Error),

    #[error("I/O error: {0:?}")]
    IoError(#[from] io::Error),

    #[error("Serde error: {0:?}")]
    SerdeError(#[from] serde_json::Error),

    #[error("SSRI error: {0:?}")]
    SSRIError(#[from] chaste_types::SSRIError),

    #[error("Path lexing error: {0:?}")]
    LogosError(#[from] PathLexingError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
