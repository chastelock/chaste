// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

#![no_main]

use libfuzzer_sys::fuzz_target;

use chaste_types::SourceVersionSpecifier;

fuzz_target!(|data: String| {
    if let Ok(svs) = SourceVersionSpecifier::new(data) {
        if let Some(aliased) = svs.aliased_package_name() {
            let _ = aliased.name_rest();
            let _ = aliased.scope();
            let _ = aliased.scope_name();
            let _ = aliased.scope_prefix();
        }
        let _ = svs.ssh_path_sep();
        let _ = svs.kind();
        let _ = svs.npm_range();
    }
});
