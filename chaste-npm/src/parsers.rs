// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use logos::{skip, Logos};
use thiserror::Error;

use crate::Result;

#[derive(Debug, Default, PartialEq, Eq, Clone, Error)]
#[non_exhaustive]
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

pub(crate) fn package_name_from_path<'a>(path: &'a str) -> Result<Option<&'a str>> {
    let path_tokens = PathToken::lexer(path)
        .spanned()
        .map(|(rpt, s)| Ok((rpt?, s)))
        .collect::<Result<Vec<(PathToken, logos::Span)>, PathLexingError>>()?;
    if let Some(nm_index) = path_tokens
        .iter()
        .rposition(|(t, _)| *t == PathToken::NodeModules)
    {
        let (_, logos::Span { start, mut end }) = path_tokens[nm_index + 1];
        // the name is under a scope like this: "@scope/name"
        if path[start..start + 1] == *"@" {
            (_, logos::Span { end, .. }) = path_tokens[nm_index + 2];
        }
        Ok(Some(&path[start..end]))
    } else {
        Ok(None)
    }
}
