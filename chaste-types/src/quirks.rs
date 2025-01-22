// SPDX-FileCopyrightText: 2025 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

/// Sometimes behavior is a bit different between implementations.
/// This is a pleister type to stick on top of these problems.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum QuirksMode {
    Yarn(u8),
}
