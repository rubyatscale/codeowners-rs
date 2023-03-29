use assert_cmd::prelude::*;
use std::{error::Error, process::Command};

#[test]
fn test_verify() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("verify")
        .assert()
        .success();

    Ok(())
}
