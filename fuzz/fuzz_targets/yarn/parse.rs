// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

#![no_main]

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Debug)]
struct NotAnError;

impl std::fmt::Display for NotAnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotAnError")
    }
}

impl std::error::Error for NotAnError {}

/// ::NotFound omitted, returned if nothing was generated
static IO_ERROR_KIND: [io::ErrorKind; 7] = [
    io::ErrorKind::FileTooLarge,
    io::ErrorKind::InvalidData,
    io::ErrorKind::InvalidFilename,
    io::ErrorKind::IsADirectory,
    io::ErrorKind::PermissionDenied,
    io::ErrorKind::TooManyLinks,
    io::ErrorKind::Unsupported,
];

#[derive(Debug)]
struct ArbitraryIoErrorKind(io::ErrorKind);

impl Arbitrary<'_> for ArbitraryIoErrorKind {
    fn arbitrary(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<Self> {
        Ok(ArbitraryIoErrorKind(*u.choose(&IO_ERROR_KIND)?))
    }
}

#[derive(Debug, Arbitrary)]
struct Data {
    lockfile: String,
    files: HashMap<PathBuf, Result<String, ArbitraryIoErrorKind>>,
}

fuzz_target!(|data: Data| {
    let _ = chaste_yarn::parse_arbitrary(&data.lockfile, Path::new(""), &|p| {
        data.files
            .get(&p)
            .map(|o| {
                o.as_ref()
                    .map(|s| s.clone())
                    .map_err(|e| io::Error::new(e.0, NotAnError))
            })
            .unwrap_or_else(|| Err(io::Error::new(io::ErrorKind::NotFound, NotAnError)))
    });
});
