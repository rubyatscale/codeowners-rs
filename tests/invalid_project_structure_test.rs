use assert_cmd::prelude::*;
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

#[test]
fn test_invalid_project_root() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/invalid_project_root")
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Can't canonicalize project root"));

    Ok(())
}
