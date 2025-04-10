// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::io;

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Unknown lockfile version: {0:?}")]
    UnknownLockfileVersion(String),

    #[error("Missing root importer")]
    MissingRootImporter,

    #[error("Package {0:?} not found, marked as a dependency")]
    DependencyPackageNotFound(String),

    #[error("Could not parse package descriptor: {0:?}")]
    InvalidPackageDescriptor(String),

    #[error("Could not parse snapshot descriptor: {0:?}")]
    InvalidSnapshotDescriptor(String),

    #[error("Could not parse the specifier of a patched package: {0:?}")]
    InvalidPatchedPackageSpecifier(String),

    #[error("Invalid patch hash: {0:?}")]
    InvalidPatchHash(String),

    #[error("Chaste error: {0:?}")]
    ChasteError(#[from] chaste_types::Error),

    #[error("I/O error: {0:?}")]
    IoError(#[from] io::Error),

    #[error("Serde JSON error: {0:?}")]
    JSONError(#[from] serde_json::Error),

    #[error("Serde Norway error: {0:?}")]
    NorwayError(#[from] serde_norway::Error),

    #[error("SSRI error: {0:?}")]
    SSRIError(#[from] chaste_types::SSRIError),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
