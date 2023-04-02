use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{error::Error, process::Command};

#[test]
fn test_validate() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/invalid_project")
        .arg("validate")
        .assert()
        .failure()
        .stdout(predicate::str::contains("CODEOWNERS out of date. Run `codeownership generate` to update the CODEOWNERS file"))
        .stdout(predicate::str::contains("Some files are missing ownership\n- ruby/app/unowned.rb"))
        .stdout(predicate::str::contains("Code ownership should only be defined for each file in one way. The following files have declared ownership in multiple ways.\n- gems/payroll_calculator/calculator.rb (owner: Payments, source: team_file_mapper)\n- gems/payroll_calculator/calculator.rb (owner: Payroll, source: team_gem_mapper)"));

    Ok(())
}