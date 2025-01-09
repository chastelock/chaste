<!--
SPDX-FileCopyrightText: 2025 The Chaste Authors
SPDX-License-Identifier: CC0-1.0
-->

Chaste parses npmjs lockfiles.

Development status: alpha.

## `chaste why`

"Why does my tree depend on this package?"

```
$ chaste why is-number
@chastelock/testcase -Dependency-> is-even -Dependency-> is-odd -Dependency-> is-number
```

***

* Main crate: [`chaste` crate](https://crates.io/crates/chaste)
* CLI: [`chaste-cli` crate](https://crates.io/crates/chaste-cli)
* Types package: [`chaste-types` crate](https://crates.io/crates/chaste-types)
* npm implementation: [`chaste-npm` crate](https://crates.io/crates/chaste-npm)
* pnpm implementation: [`chaste-pnpm` crate](https://crates.io/crates/chaste-pnpm)
* yarn implementation: [`chaste-yarn` crate](https://crates.io/crates/chaste-yarn)
