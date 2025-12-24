use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::predicate;
use std::{error::Error, fs, path::Path, process::Command};

mod common;

use common::OutputStream;
use common::run_codeowners;

#[test]
fn test_validate() -> Result<(), Box<dyn Error>> {
    run_codeowners("valid_project", &["validate"], true, OutputStream::Stdout, predicate::eq(""))?;

    Ok(())
}

#[test]
fn test_generate() -> Result<(), Box<dyn Error>> {
    fs::create_dir_all("tmp")?;
    let codeowners_abs = std::env::current_dir()?.join("tmp/CODEOWNERS");
    let codeowners_str = codeowners_abs.to_str().unwrap();

    run_codeowners(
        "valid_project",
        &["--codeowners-file-path", codeowners_str, "generate"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    let expected_codeowners: String = std::fs::read_to_string(Path::new("tests/fixtures/valid_project/.github/CODEOWNERS"))?;
    let actual_codeowners: String = std::fs::read_to_string(Path::new("tmp/CODEOWNERS"))?;

    assert_eq!(expected_codeowners, actual_codeowners);

    Ok(())
}

#[test]
fn test_crosscheck_owners() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project",
        &["crosscheck-owners"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {"
            Success! All files match between CODEOWNERS and for-file command.
        "}),
    )?;

    Ok(())
}

#[test]
fn test_for_file() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project",
        &["for-file", "ruby/app/models/payroll.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {"
            Team: Payroll
            Github Team: @PayrollTeam
            Team YML: config/teams/payroll.yml
            Description:
            - Owner annotation at the top of the file
        "}),
    )?;

    Ok(())
}

#[test]
fn test_for_file_json() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project",
        &["for-file", "ruby/app/models/payroll.rb", "--json"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {r#"
            {
              "team_name": "Payroll",
              "github_team": "@PayrollTeam",
              "team_yml": "config/teams/payroll.yml",
              "description": [
                "Owner annotation at the top of the file"
              ]
            }
        "#}),
    )?;

    Ok(())
}

#[test]
fn test_for_file_full_path() -> Result<(), Box<dyn Error>> {
    let project_root = Path::new("tests/fixtures/valid_project");
    let for_file_absolute_path = fs::canonicalize(project_root.join("ruby/app/models/payroll.rb"))?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("for-file")
        .arg(for_file_absolute_path.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Payroll
            Github Team: @PayrollTeam
            Team YML: config/teams/payroll.yml
            Description:
            - Owner annotation at the top of the file
        "}));
    Ok(())
}

#[test]
fn test_for_file_full_path_json() -> Result<(), Box<dyn Error>> {
    let project_root = Path::new("tests/fixtures/valid_project");
    let for_file_absolute_path = fs::canonicalize(project_root.join("ruby/app/models/payroll.rb"))?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("for-file")
        .arg(for_file_absolute_path.to_str().unwrap())
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {r#"
            {
              "team_name": "Payroll",
              "github_team": "@PayrollTeam",
              "team_yml": "config/teams/payroll.yml",
              "description": [
                "Owner annotation at the top of the file"
              ]
            }
        "#}));
    Ok(())
}

#[test]
fn test_fast_for_file() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("ruby/app/models/payroll.rb")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Payroll
            Github Team: @PayrollTeam
            Team YML: config/teams/payroll.yml
            Description:
            - Owner annotation at the top of the file
        "}));
    Ok(())
}

#[test]
fn test_fast_for_file_with_ignored_file() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("should_be_ignored/an_ignored_file.rb")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Unowned
            Github Team: Unowned
            Team YML: 
            Description:
        "}));
    Ok(())
}

#[test]
fn test_fast_for_file_full_path() -> Result<(), Box<dyn Error>> {
    let project_root = Path::new("tests/fixtures/valid_project");
    let for_file_absolute_path = fs::canonicalize(project_root.join("ruby/app/models/payroll.rb"))?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("for-file")
        .arg(for_file_absolute_path.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Payroll
            Github Team: @PayrollTeam
            Team YML: config/teams/payroll.yml
            Description:
            - Owner annotation at the top of the file
        "}));
    Ok(())
}

#[test]
fn test_for_file_with_components() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("gems/pets/dog.rb")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: UX
            Github Team: @UX
            Team YML: config/teams/ux.yml
            Description:
            - Owner specified in Team YML's `owned_gems`
        "}));
    Ok(())
}

#[test]
fn test_for_file_same_team_multiple_ownerships() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("javascript/packages/PayrollFlow/index.tsx")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Payroll
            Github Team: @PayrollTeam
            Team YML: config/teams/payroll.yml
            Description:
            - Owner annotation at the top of the file
            - Owner defined in `javascript/packages/PayrollFlow/package.json` with implicity owned glob: `javascript/packages/PayrollFlow/**/**`
        "}));
    Ok(())
}

#[test]
fn test_fast_for_file_same_team_multiple_ownerships() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("javascript/packages/PayrollFlow/index.tsx")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Payroll
            Github Team: @PayrollTeam
            Team YML: config/teams/payroll.yml
            Description:
            - Owner annotation at the top of the file
            - Owner defined in `javascript/packages/PayrollFlow/package.json` with implicity owned glob: `javascript/packages/PayrollFlow/**/**`
        "}));
    Ok(())
}

#[test]
fn test_for_file_with_2_ownerships() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("javascript/packages/PayrollFlow/index.tsx")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Payroll
            Github Team: @PayrollTeam
            Team YML: config/teams/payroll.yml
            Description:
            - Owner annotation at the top of the file
            - Owner defined in `javascript/packages/PayrollFlow/package.json` with implicity owned glob: `javascript/packages/PayrollFlow/**/**`
        "}));

    Ok(())
}

#[test]
fn test_for_team() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "valid_project",
        &["for-team", "Payroll"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {"
            # Code Ownership Report for `Payroll` Team

            ## Owned Files
            /config/teams/payroll.yml
            /gems/payroll_calculator/**/**
            /javascript/packages/PayrollFlow/**/**
            /javascript/packages/PayrollFlow/index.tsx
            /javascript/packages/items/**/**
            /ruby/app/models/payroll.rb
            /ruby/app/payments/foo/**/**
            /ruby/app/payroll/**/**
            /ruby/app/views/foos/edit.erb
            /ruby/app/views/foos/new.html.erb
            /ruby/packages/payroll_flow/**/**
        "}),
    )?;

    Ok(())
}

#[test]
fn test_for_missing_team() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("--no-cache")
        .arg("for-team")
        .arg("Nope")
        .assert()
        .failure()
        .stderr(predicate::eq(indoc! {"
            Team not found
        "}));

    Ok(())
}
