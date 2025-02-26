use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, process::Command};

#[test]
fn test_no_config_file() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures")
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Can't open config file"));

    Ok(())
}