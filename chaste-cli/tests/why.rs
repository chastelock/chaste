// SPDX-FileCopyrightText: 2024 The Chaste Authors
// SPDX-License-Identifier: Apache-2.0 OR BSD-2-Clause

use anyhow::Result;
use assert_cmd::Command;

#[test]
#[cfg(feature = "npm")]
fn npm_v3_infinite_recursion() -> Result<()> {
    Command::cargo_bin("chaste")?
        .args(["why", "@chastelock/recursion-a"])
        .current_dir("test_workspaces/npm_v3_infinite_recursion")
        .assert()
        .success()
        .stdout("@chastelock/testcase -Dependency-> @chastelock/recursion-a\n");

    Command::cargo_bin("chaste")?
        .args(["why", "@chastelock/recursion-b"])
        .current_dir("test_workspaces/npm_v3_infinite_recursion")
        .assert()
        .success()
        .stdout("@chastelock/testcase -Dependency-> @chastelock/recursion-a -Dependency-> @chastelock/recursion-b\n");

    Ok(())
}

#[test]
#[cfg(feature = "yarn-classic")]
fn yarn_v1_infinite_recursion() -> Result<()> {
    Command::cargo_bin("chaste")?
        .args(["why", "@chastelock/recursion-a"])
        .current_dir("test_workspaces/yarn_v1_infinite_recursion")
        .assert()
        .success()
        .stdout("@chastelock/testcase -Dependency-> @chastelock/recursion-a\n");

    Command::cargo_bin("chaste")?
        .args(["why", "@chastelock/recursion-b"])
        .current_dir("test_workspaces/yarn_v1_infinite_recursion")
        .assert()
        .success()
        .stdout("@chastelock/testcase -Dependency-> @chastelock/recursion-a -Dependency-> @chastelock/recursion-b\n");

    Ok(())
}
