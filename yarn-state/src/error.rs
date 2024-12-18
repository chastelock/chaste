// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("Invalid .yarn-state.yml syntax")]
    InvalidSyntax(),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
