<!--
SPDX-FileCopyrightText: 2025 The Chaste Authors
SPDX-License-Identifier: CC0-1.0
-->

Chaste parses Bun lockfiles.

Development status: alpha.

This crate contains the implementation for Bun only: The text-based `bun.lock` format.
No support for `bun.lockb` or `yarn.lock` (for that one, see the [`chaste-yarn` crate](https://crates.io/crates/chaste-yarn)).
You're probably interested in the [`chaste` crate](https://crates.io/crates/chaste),
which re-exposes this crate.

Documentation: https://docs.rs/chaste-bun

* Main crate: [`chaste` crate](https://crates.io/crates/chaste)
* CLI: [`chaste-cli` crate](https://crates.io/crates/chaste-cli)
* Types package: [`chaste-types` crate](https://crates.io/crates/chaste-types)
* Bun implementation: [`chaste-bun` crate](https://crates.io/crates/chaste-bun)
* npm implementation: [`chaste-npm` crate](https://crates.io/crates/chaste-npm)
* pnpm implementation: [`chaste-pnpm` crate](https://crates.io/crates/chaste-pnpm)
* yarn implementation: [`chaste-yarn` crate](https://crates.io/crates/chaste-yarn)
