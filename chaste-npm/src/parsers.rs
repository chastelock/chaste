// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use chaste_types::PackageName;
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

// TODO: Replace this when https://codeberg.org/selfisekai/chaste/issues/11 is done.
pub(crate) fn package_name_from_path(path: &str) -> Result<Option<&str>> {
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

    // This is an edge case found via v1_workspace_basic.
    } else if PackageName::new(path.to_string()).is_ok() {
        Ok(Some(path))
    } else {
        Ok(None)
    }
}
