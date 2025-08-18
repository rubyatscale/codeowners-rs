use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};

mod common;
use common::setup_fixture_repo;

const FIXTURE: &str = "tests/fixtures/valid_project";

#[test]
fn test_crosscheck_owners_reports_team_mismatch() -> Result<(), Box<dyn Error>> {
    // Arrange: copy fixture to temp dir and change a single CODEOWNERS mapping
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    let codeowners_path = project_root.join(".github/CODEOWNERS");

    let original = fs::read_to_string(&codeowners_path)?;
    // Change payroll.rb ownership from @PayrollTeam to @PaymentsTeam to induce a mismatch
    let modified = original.replace(
        "/ruby/app/models/payroll.rb @PayrollTeam",
        "/ruby/app/models/payroll.rb @PaymentsTeam",
    );
    fs::write(&codeowners_path, modified)?;

    // Act + Assert
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("crosscheck-owners")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            indoc! {"- ruby/app/models/payroll.rb: CODEOWNERS=Payments fast=Payroll"},
        ));

    Ok(())
}

#[test]
fn test_crosscheck_owners_reports_unowned_mismatch() -> Result<(), Box<dyn Error>> {
    // Arrange: copy fixture to temp dir and remove a CODEOWNERS rule for an owned file
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    let codeowners_path = project_root.join(".github/CODEOWNERS");

    // Remove the explicit mapping for bank_account.rb so CODEOWNERS reports Unowned
    let original = fs::read_to_string(&codeowners_path)?;
    let modified: String = original
        .lines()
        .filter(|line| !line.trim().starts_with("/ruby/app/models/bank_account.rb "))
        .map(|l| format!("{}\n", l))
        .collect();
    fs::write(&codeowners_path, modified)?;

    // Act + Assert
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("crosscheck-owners")
        .assert()
        .failure()
        .stdout(predicate::str::contains(
            "- ruby/app/models/bank_account.rb: CODEOWNERS=Unowned fast=Payments",
        ));

    Ok(())
}
