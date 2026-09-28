use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};

mod common;
use common::{OutputStream, git_add_all_files, run_codeowners, setup_fixture_repo};

const FIXTURE: &str = "tests/fixtures/allow_unowned_files";

#[test]
fn test_validate_passes_with_unowned_files() -> Result<(), Box<dyn Error>> {
    run_codeowners("allow_unowned_files", &["validate"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_generate_and_validate_passes_with_unowned_files() -> Result<(), Box<dyn Error>> {
    run_codeowners("allow_unowned_files", &["gv"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_validate_files_passes_for_an_unowned_file() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "allow_unowned_files",
        &["validate", "app/unowned.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;
    Ok(())
}

#[test]
fn test_annotations_still_assign_owners() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "allow_unowned_files",
        &["for-file", "app/annotated.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {"
            Team: Foo
            Github Team: @FooTeam
            Team YML: config/teams/foo.yml
            Description:
            - Owner annotation at the top of the file
        "}),
    )?;
    Ok(())
}

#[test]
fn test_unowned_file_is_reported_as_unowned() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "allow_unowned_files",
        &["for-file", "app/unowned.rb"],
        true,
        OutputStream::Stdout,
        predicate::str::contains("Team: Unowned"),
    )?;
    Ok(())
}

fn run_without_the_flag(args: &[&str]) -> Result<assert_cmd::assert::Assert, Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    fs::write(
        project_root.join("config/code_ownership.yml"),
        "---\nowned_globs:\n  - \"{app,config}/**/*.rb\"\n",
    )?;
    git_add_all_files(project_root);
    let assert = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .args(args)
        .assert();
    Ok(assert)
}

#[test]
fn test_validate_reports_unowned_files_without_the_flag() -> Result<(), Box<dyn Error>> {
    run_without_the_flag(&["validate"])?
        .failure()
        .stdout(predicate::str::contains("Some files are missing ownership"))
        .stdout(predicate::str::contains("- app/unowned.rb"));
    Ok(())
}

#[test]
fn test_validate_files_reports_an_unowned_file_without_the_flag() -> Result<(), Box<dyn Error>> {
    run_without_the_flag(&["validate", "app/unowned.rb"])?
        .failure()
        .stdout(predicate::str::contains("Unowned files detected:"))
        .stdout(predicate::str::contains("app/unowned.rb"));
    Ok(())
}
