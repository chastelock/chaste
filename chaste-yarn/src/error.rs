// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::{io, str};

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Dependency {0:?} not found")]
    DependencyNotFound(String),

    #[error("The root package uses workspaces. This is not handled yet")]
    RootHasWorkspaces(),

    #[error("Unknown yarn.lock version: {0:?}")]
    UnknownLockfileVersion(u8),

    #[error("Chaste error: {0:?}")]
    ChasteError(#[from] chaste_types::Error),

    #[error("I/O error: {0:?}")]
    IoError(#[from] io::Error),

    #[error("UTF-8 parsing error: {0:?}")]
    Utf8Error(#[from] str::Utf8Error),

    #[error("Yarn parser error: {0:?}")]
    YarnParserError(#[from] yarn_lock_parser::YarnLockError),

    #[error("SSRI error: {0:?}")]
    SSRIError(#[from] chaste_types::SSRIError),

    #[error("JSON parsing error: {0:?}")]
    SerdeJsonError(#[from] serde_json::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
