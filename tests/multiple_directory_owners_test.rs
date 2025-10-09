use std::{error::Error, path::Path};

use common::run_codeowners;
use common::OutputStream;
use predicates::prelude::*;

mod common;

#[test]
fn test_validate() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "multiple-directory-owners",
        &["validate"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    Ok(())
}

#[test]
fn test_generate() -> Result<(), Box<dyn Error>> {
    let codeowners_abs = std::env::current_dir()?.join("tmp/CODEOWNERS");
    let codeowners_str = codeowners_abs.to_str().unwrap();

    run_codeowners(
        "multiple-directory-owners",
        &["--codeowners-file-path", codeowners_str, "generate"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    let expected_codeowners: String = std::fs::read_to_string(Path::new("tests/fixtures/multiple-directory-owners/.github/CODEOWNERS"))?;
    let actual_codeowners: String = std::fs::read_to_string(Path::new("tmp/CODEOWNERS"))?;

    assert_eq!(expected_codeowners, actual_codeowners);

    Ok(())
}
