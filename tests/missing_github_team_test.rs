use predicates::prelude::*;
use std::error::Error;

mod common;
use common::OutputStream;
use common::run_codeowners;

// Exercise the code path that skips invalid team files and prints to stderr
// (codeowners_file_parser::teams_by_github_team_name). Uses for-file
// --from-codeowners so the project is not built and the parser globs team
// files; the invalid bad_team.yml is skipped and an error is printed.
// With the fix: stderr contains "Error parsing team file:" and "missing field `github`".
// Without the fix (reverted): stderr only has generic "YAML serialization/deserialization failed".
#[test]
fn test_missing_github_team_in_team_file_is_reported_on_stderr() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "missing_github_team",
        &["for-file", "--from-codeowners", "ruby/foo.rb"],
        true, // command succeeds; invalid file is skipped
        OutputStream::Stderr,
        predicate::str::contains("Error parsing team file:").and(predicate::str::contains("missing field `github`")),
    )
}
