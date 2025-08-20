use indoc::indoc;
use predicates::prelude::predicate;
use std::error::Error;

mod common;

use common::OutputStream;
use common::run_codeowners;

#[test]
#[ignore]
fn test_validate() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project_with_overrides",
        &["validate"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    Ok(())
}

#[test]
#[ignore]
fn test_crosscheck_owners() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project_with_overrides",
        &["crosscheck-owners"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {"
            Success! All files match between CODEOWNERS and for-file command.
        "}),
    )?;

    Ok(())
}
