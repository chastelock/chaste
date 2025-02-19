<!--
SPDX-FileCopyrightText: 2025 The Chaste Authors
SPDX-License-Identifier: CC0-1.0
-->

Chaste parses npmjs lockfiles.

Development status: alpha.

This crate exports everything at one place:
- returned types: same unified format for all package managers,
- parser for Bun's bun.lock (`bun` feature),
- parser for npm's package-lock.json (`npm` feature),
- parser for pnpm's pnpm-lock.yaml (`pnpm` feature),
- parser for yarn's (both Classic and Berry) yarn.lock (`yarn` feature).

Documentation: https://docs.rs/chaste

* Main crate: [`chaste` crate](https://crates.io/crates/chaste)
* CLI: [`chaste-cli` crate](https://crates.io/crates/chaste-cli)
* Types package: [`chaste-types` crate](https://crates.io/crates/chaste-types)
* Bun implementation: [`chaste-bun` crate](https://crates.io/crates/chaste-bun)
* npm implementation: [`chaste-npm` crate](https://crates.io/crates/chaste-npm)
* pnpm implementation: [`chaste-pnpm` crate](https://crates.io/crates/chaste-pnpm)
* yarn implementation: [`chaste-yarn` crate](https://crates.io/crates/chaste-yarn)
