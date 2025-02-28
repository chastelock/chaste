// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

#![no_main]

use libfuzzer_sys::fuzz_target;

use chaste_types::ModulePath;

fuzz_target!(|data: String| {
    if let Ok(path) = ModulePath::new(data) {
        let _ = path.implied_package_name();
        for segment in path.iter() {
            let _ = segment.as_ref();
        }
    }
});
