// SPDX-FileCopyrightText: 2026 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum LockfileVersion<'m> {
    U8(u8),
    Str(&'m str),
}

impl<'m> fmt::Display for LockfileVersion<'m> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockfileVersion::U8(i) => i.fmt(f),
            LockfileVersion::Str(i) => i.fmt(f),
        }
    }
}

pub trait ProviderMeta {
    fn provider_name(&self) -> &'static str;
    fn lockfile_version<'m>(&'m self) -> Option<LockfileVersion<'m>>;
}

impl ProviderMeta for () {
    fn provider_name(&self) -> &'static str {
        "()"
    }

    fn lockfile_version<'m>(&'m self) -> Option<LockfileVersion<'m>> {
        None
    }
}
