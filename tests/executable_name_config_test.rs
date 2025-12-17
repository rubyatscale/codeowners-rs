use indoc::indoc;
use predicates::prelude::*;
use std::error::Error;

mod common;
use common::OutputStream;
use common::run_codeowners;

#[test]
fn test_validate_with_custom_executable_name() -> Result<(), Box<dyn Error>> {
    // When executable_name is configured, error should show that command
    run_codeowners(
        "custom_executable_name",
        &["validate"],
        false,
        OutputStream::Stdout,
        predicate::str::contains("Run `bin/codeownership generate`"),
    )?;
    Ok(())
}

#[test]
fn test_validate_with_default_executable_name() -> Result<(), Box<dyn Error>> {
    // When executable_name is not configured, error should show default "codeowners"
    run_codeowners(
        "default_executable_name",
        &["validate"],
        false,
        OutputStream::Stdout,
        predicate::str::contains("Run `codeowners generate`"),
    )?;
    Ok(())
}

#[test]
fn test_custom_executable_name_full_error_message() -> Result<(), Box<dyn Error>> {
    // Verify the complete error message format with custom executable
    run_codeowners(
        "custom_executable_name",
        &["validate"],
        false,
        OutputStream::Stdout,
        predicate::eq(indoc! {"

    CODEOWNERS out of date. Run `bin/codeownership generate` to update the CODEOWNERS file

    "}),
    )?;
    Ok(())
}

#[test]
fn test_default_executable_name_full_error_message() -> Result<(), Box<dyn Error>> {
    // Verify the complete error message format with default executable
    run_codeowners(
        "default_executable_name",
        &["validate"],
        false,
        OutputStream::Stdout,
        predicate::eq(indoc! {"

    CODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file

    "}),
    )?;
    Ok(())
}
