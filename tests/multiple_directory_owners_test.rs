use assert_cmd::prelude::*;
use std::{error::Error, path::Path, process::Command};

#[test]
fn test_validate() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/multiple-directory-owners")
        .arg("--no-cache")
        .arg("validate")
        .assert()
        .success();

    Ok(())
}

#[test]
fn test_generate() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/multiple-directory-owners")
        .arg("--codeowners-file-path")
        .arg("../../../tmp/CODEOWNERS")
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();

    let expected_codeowners: String = std::fs::read_to_string(Path::new("tests/fixtures/multiple-directory-owners/.github/CODEOWNERS"))?;
    let actual_codeowners: String = std::fs::read_to_string(Path::new("tmp/CODEOWNERS"))?;

    assert_eq!(expected_codeowners, actual_codeowners);

    Ok(())
}
