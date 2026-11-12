use indoc::indoc;
use predicates::prelude::*;
use std::error::Error;

mod common;
use common::OutputStream;
use common::run_codeowners;

/// `owner` is `content.trim()` (`project_builder.rs:278`), so an empty or whitespace-only
/// `.codeowner` yields `""` and hits the same silent-inheritance path as a typo'd name.
/// Reporting it as `an invalid team - ''` would give no hint the file is empty.
#[test]
fn test_validate_reports_empty_directory_codeowner() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "empty-directory-codeowner",
        &["validate"],
        false,
        OutputStream::Stdout,
        predicate::str::contains(indoc! {"
            Found invalid team references
            - app/services/nested/.codeowner is empty and names no team; this directory is currently inheriting its owner from app/services
        "}),
    )?;

    Ok(())
}
