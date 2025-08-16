use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::predicate;
use std::{error::Error, process::Command};

#[test]
fn test_validate() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project_with_overrides")
        .arg("--no-cache")
        .arg("validate")
        .assert()
        .success();

    Ok(())
}

#[test]
fn test_verify_compare_for_file() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project_with_overrides")
        .arg("--no-cache")
        .arg("verify-compare-for-file")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Success! All files match between CODEOWNERS and for-file command.
        "}));

    Ok(())
}
