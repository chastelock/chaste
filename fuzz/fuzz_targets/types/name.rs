// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

#![no_main]

use libfuzzer_sys::fuzz_target;

use chaste_types::PackageName;

fuzz_target!(|data: String| {
    if let Ok(name) = PackageName::new(data) {
        let _ = name.name_rest();
        let _ = name.scope();
        let _ = name.scope_name();
        let _ = name.scope_prefix();
        let borrowed = name.as_borrowed();
        let _ = borrowed.name_rest();
        let _ = borrowed.scope();
        let _ = borrowed.scope_name();
        let _ = borrowed.scope_prefix();
    }
});
