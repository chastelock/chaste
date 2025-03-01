// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

#![no_main]

use libfuzzer_sys::fuzz_target;

use chaste_npm::PackageLock;

fuzz_target!(|data: PackageLock| {
    let _ = chaste_npm::parse_lock(&data);
});
