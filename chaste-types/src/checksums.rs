// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

pub use ssri;
pub use ssri::{Error as SSRIError, Integrity};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checksums {
    Tarball(Integrity),
    RepackZip(Integrity),
}

impl Checksums {
    pub fn integrity(&self) -> &Integrity {
        match self {
            Checksums::Tarball(inte) => inte,
            Checksums::RepackZip(inte) => inte,
        }
    }
}
