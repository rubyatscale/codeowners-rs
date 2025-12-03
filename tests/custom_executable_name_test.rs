use predicates::prelude::*;
use std::error::Error;

mod common;
use common::OutputStream;
use common::run_codeowners;

#[test]
fn test_validate_uses_custom_executable_name() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "custom_executable_name",
        &["validate"],
        false,
        OutputStream::Stdout,
        predicate::str::contains("CODEOWNERS out of date. Run `my-custom-tool generate` to update the CODEOWNERS file"),
    )?;
    Ok(())
}

#[test]
fn test_validate_default_executable_name() -> Result<(), Box<dyn Error>> {
    // This tests the invalid_project which doesn't specify executable_name
    // and should use the default "codeowners"
    run_codeowners(
        "invalid_project",
        &["validate"],
        false,
        OutputStream::Stdout,
        predicate::str::contains("CODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file"),
    )?;
    Ok(())
}
