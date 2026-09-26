use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;
use std::error::Error;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

mod common;
use common::OutputStream;
use common::git_add_all_files;
use common::run_codeowners;
use common::setup_fixture_repo;

// Alpha owns ruby/app/**/* but excludes beta_owned.rb through unowned_globs, and Beta owns that file
// through an annotation. GitHub applies the last matching CODEOWNERS line, so the annotation line must
// come after Alpha's broader glob for GitHub to agree with for-file.
const FIXTURE: &str = "annotation_in_unowned_team_glob";

// Copies the fixture and regenerates its CODEOWNERS, so assertions exercise the generator rather than
// the committed file.
fn fixture_with_generated_codeowners() -> Result<TempDir, Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(&Path::new("tests/fixtures").join(FIXTURE));
    git_add_all_files(temp_dir.path());
    codeowners(temp_dir.path(), &["generate"])?.assert().success();
    Ok(temp_dir)
}

fn codeowners(project_root: &Path, args: &[&str]) -> Result<Command, Box<dyn Error>> {
    let mut cmd = Command::cargo_bin("codeowners")?;
    cmd.arg("--project-root").arg(project_root).arg("--no-cache").args(args);
    Ok(cmd)
}

#[test]
fn test_validate_accepts_generated_order() -> Result<(), Box<dyn Error>> {
    run_codeowners(FIXTURE, &["validate"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_generated_codeowners_agrees_with_for_file() -> Result<(), Box<dyn Error>> {
    let temp_dir = fixture_with_generated_codeowners()?;
    codeowners(temp_dir.path(), &["crosscheck-owners"])?
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Success! All files match between CODEOWNERS and for-file command.
        "}));
    Ok(())
}

#[test]
fn test_for_file_from_generated_codeowners_returns_annotated_owner() -> Result<(), Box<dyn Error>> {
    let temp_dir = fixture_with_generated_codeowners()?;
    codeowners(
        temp_dir.path(),
        &["for-file", "--from-codeowners", "ruby/app/services/beta_owned.rb"],
    )?
    .assert()
    .success()
    .stdout(predicate::str::contains("Team: Beta"));
    Ok(())
}
