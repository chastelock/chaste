// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::io;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Unknown lockfile version: {0}")]
    UnknownLockfileVersion(u8),

    #[error("Invalid package key: {0:?}")]
    InvalidKey(String),

    #[error("Invalid package descriptor: {0:?}")]
    InvalidDescriptor(String),

    #[error("Dependency {0:?} not found")]
    DependencyNotFound(String),

    #[error("Invalid package data variant in key {0:?}")]
    InvalidVariant(String),

    #[error("Data variant mismatched with source/version marker in key {0:?}")]
    VariantMarkerMismatch(String),

    #[error("I/O error: {0:?}")]
    IOError(#[from] io::Error),

    #[error("JSONC error: {0:?}")]
    JSONCError(#[from] json5::Error),

    #[error("Chaste error: {0:?}")]
    ChasteError(#[from] chaste_types::Error),

    #[error("Chaste error: {0:?}")]
    SSRIError(#[from] chaste_types::ssri::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
