use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{error::Error, process::Command};

mod common;

use common::*;

#[test]
fn test_validate_with_owned_files() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project",
        &["validate", "ruby/app/models/payroll.rb", "ruby/app/models/bank_account.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    Ok(())
}

#[test]
fn test_validate_with_unowned_file() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project",
        &["validate", "ruby/app/unowned.rb"],
        false,
        OutputStream::Stdout,
        predicate::str::contains("ruby/app/unowned.rb").and(predicate::str::contains("Unowned")),
    )?;

    Ok(())
}

#[test]
fn test_validate_with_mixed_files() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project",
        &["validate", "ruby/app/models/payroll.rb", "ruby/app/unowned.rb"],
        false,
        OutputStream::Stdout,
        predicate::str::contains("ruby/app/unowned.rb").and(predicate::str::contains("Unowned")),
    )?;

    Ok(())
}

#[test]
fn test_validate_with_no_files() -> Result<(), Box<dyn Error>> {
    // Existing behavior - validates entire project
    run_codeowners("valid_project", &["validate"], true, OutputStream::Stdout, predicate::eq(""))?;

    Ok(())
}

#[test]
fn test_generate_and_validate_with_owned_files() -> Result<(), Box<dyn Error>> {
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("generate-and-validate")
        .arg("ruby/app/models/payroll.rb")
        .arg("ruby/app/models/bank_account.rb")
        .assert()
        .success();

    Ok(())
}

#[test]
fn test_generate_and_validate_with_unowned_file() -> Result<(), Box<dyn Error>> {
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("generate-and-validate")
        .arg("ruby/app/unowned.rb")
        .assert()
        .failure()
        .stdout(predicate::str::contains("ruby/app/unowned.rb"))
        .stdout(predicate::str::contains("Unowned"));

    Ok(())
}

#[test]
fn test_validate_with_absolute_path() -> Result<(), Box<dyn Error>> {
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let file_absolute_path = project_root.join("ruby/app/models/payroll.rb").canonicalize()?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("validate")
        .arg(file_absolute_path.to_str().unwrap())
        .assert()
        .success();

    Ok(())
}

#[test]
fn test_validate_only_checks_codeowners_file() -> Result<(), Box<dyn Error>> {
    // This test demonstrates that `validate` with files only checks the CODEOWNERS file
    // It does NOT check file annotations or other ownership sources
    //
    // If a file has an annotation but is missing from CODEOWNERS, `validate` will report it as unowned
    // This is why `generate-and-validate` should be used for accuracy

    // ruby/app/models/bank_account.rb has @team Payments annotation and is in CODEOWNERS
    run_codeowners(
        "valid_project",
        &["validate", "ruby/app/models/bank_account.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    Ok(())
}
