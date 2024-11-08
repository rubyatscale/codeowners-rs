use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::predicate;
use std::{error::Error, path::Path, process::Command};

#[test]
fn test_validate() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("validate")
        .assert()
        .success();

    Ok(())
}

#[test]
fn test_generate() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("--codeowners-file-path")
        .arg("../../../tmp/CODEOWNERS")
        .arg("generate")
        .assert()
        .success();

    let expected_codeowners: String = std::fs::read_to_string(Path::new("tests/fixtures/valid_project/.github/CODEOWNERS"))?;
    let actual_codeowners: String = std::fs::read_to_string(Path::new("tmp/CODEOWNERS"))?;

    assert_eq!(expected_codeowners, actual_codeowners);

    Ok(())
}

#[test]
fn test_for_file() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("for-file")
        .arg("ruby/app/models/payroll.rb")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Payroll
            Team YML: config/teams/payroll.yml
            Description: Owner annotation at the top of the file
        "}));
    Ok(())
}

#[test]
fn test_for_file_same_team_multiple_ownerships() -> Result<(), Box<dyn Error>> {
    let expected_output = r#"
Team: Payroll
Team YML: config/teams/payroll.yml
Description:
- Owner annotation at the top of the file
- Owner defined in `javascript/packages/PayrollFlow/package.json` with implicity owned glob: `javascript/packages/PayrollFlow/**/**`
    "#
    .trim();
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("for-file")
        .arg("javascript/packages/PayrollFlow/index.tsx")
        .assert()
        .success()
        .stdout(predicate::str::contains(expected_output));
    Ok(())
}

#[test]
fn test_for_file_with_2_ownerships() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("for-file")
        .arg("javascript/packages/PayrollFlow/index.tsx")
        .assert()
        .success()
        .stdout(predicate::str::contains("Team: Payroll"))
        .stdout(predicate::str::contains("Team YML: config/teams/payroll.yml"));

    Ok(())
}

#[test]
fn test_for_team() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("for-team")
        .arg("Payroll")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            # Code Ownership Report for `Payroll` Team

            ## Annotations at the top of file
            /javascript/packages/PayrollFlow/index.tsx
            /ruby/app/models/payroll.rb

            ## Team-specific owned globs
            This team owns nothing in this category.

            ## Owner in .codeowner
            /ruby/app/payroll/**/**

            ## Owner metadata key in package.yml
            /ruby/packages/payroll_flow/**/**

            ## Owner metadata key in package.json
            /javascript/packages/PayrollFlow/**/**

            ## Team YML ownership
            /config/teams/payroll.yml

            ## Team owned gems
            /gems/payroll_calculator/**/**
        "}));
    Ok(())
}

#[test]
fn test_for_missing_team() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("for-team")
        .arg("Nope")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team not found
        "}));

    Ok(())
}
