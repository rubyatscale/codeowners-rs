use predicates::prelude::*;
use std::error::Error;

mod common;
use common::OutputStream;
use common::run_codeowners;

/// `teams_by_name` is keyed by both `name` and `github_team`, so a `.codeowner` holding
/// either form generates a correct line. Validation has to accept both, or it fails a
/// project whose CODEOWNERS is already right — with no command that fixes it.
#[test]
fn test_validate_accepts_directory_codeowner_by_name_or_github_team() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "directory-codeowner-github-team",
        &["validate"],
        true,
        OutputStream::Stdout,
        predicate::str::contains("invalid team").not(),
    )?;

    Ok(())
}
