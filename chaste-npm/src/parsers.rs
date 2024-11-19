// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use logos::{skip, Logos};
use thiserror::Error;

#[derive(Debug, Default, PartialEq, Eq, Clone, Error)]
pub enum PathLexingError {
    #[default]
    #[error("Unknown error")]
    Other,
}

#[derive(Logos, PartialEq, Eq)]
#[logos(error = PathLexingError)]
pub(crate) enum PathToken {
    #[token("/", skip)]
    Separator,

    #[token("node_modules")]
    NodeModules,

    #[regex(r"[^/]+")]
    Segment,
}
