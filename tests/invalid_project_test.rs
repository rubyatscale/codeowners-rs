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
        .stdout(predicate::str::contains("CODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file"))
        .stdout(predicate::str::contains("Some files are missing ownership\n- ruby/app/models/blockchain.rb\n- ruby/app/unowned.rb"))
        .stdout(predicate::str::contains("Found invalid team annotations\n- ruby/app/models/blockchain.rb is referencing an invalid team - 'Web3'"))
        .stdout(predicate::str::contains("Code ownership should only be defined for each file in one way. The following files have declared ownership in multiple ways\n- gems/payroll_calculator/calculator.rb (owner: Payments, source: TeamFileMapper)\n- gems/payroll_calculator/calculator.rb (owner: Payroll, source: TeamGemMapper)"));

    Ok(())
}

#[test]
fn test_for_file() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/invalid_project")
        .arg("for-file")
        .arg("ruby/app/models/blockchain.rb")
        .assert()
        .success()
        .stdout(predicate::str::contains("Team: Unowned"))
        .stdout(predicate::str::contains("Team YML: Unowned"));

    Ok(())
}

#[test]
fn test_for_file_multiple_owners() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/invalid_project")
        .arg("for-file")
        .arg("ruby/app/services/multi_owned.rb")
        .assert()
        .success()
        .stdout(predicate::str::contains("Error: file is owned by multiple teams!"))
        .stdout(predicate::str::contains("Team: Payments"))
        .stdout(predicate::str::contains("Team YML: config/teams/payments.yml"))
        .stdout(predicate::str::contains("Team: Payroll"))
        .stdout(predicate::str::contains("Team YML: config/teams/payroll.yml"));

    Ok(())
}
