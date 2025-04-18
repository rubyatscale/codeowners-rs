use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, process::Command};

#[test]
fn test_validate() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/invalid_project")
        .arg("--no-cache")
        .arg("validate")
        .assert()
        .failure()
        .stdout(predicate::eq(indoc! {"

            CODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file

            Code ownership should only be defined for each file in one way. The following files have declared ownership in multiple ways

            gems/payroll_calculator/calculator.rb
             owner: Payments
              - Owner annotation at the top of the file
             owner: Payroll
              - Owner specified in Team YML's `owned_gems`

            ruby/app/services/multi_owned.rb
             owner: Payments
              - Owner annotation at the top of the file
             owner: Payroll
              - Owner specified in `ruby/app/services/.codeowner`

            Found invalid team annotations
            - ruby/app/models/blockchain.rb is referencing an invalid team - 'Web3'

            Some files are missing ownership
            - ruby/app/models/blockchain.rb
            - ruby/app/unowned.rb

        "}));
    Ok(())
}

#[test]
fn test_for_file() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/invalid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("ruby/app/models/blockchain.rb")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Unowned
            Github Team: Unowned
            Team YML: 
            Description:
            - Unowned
            "}));
    Ok(())
}

#[test]
fn test_fast_for_file() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/invalid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("--fast")
        .arg("ruby/app/models/blockchain.rb")
        .assert()
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Unowned
            Team YML:
        "}));
    Ok(())
}

#[test]
fn test_for_file_multiple_owners() -> Result<(), Box<dyn Error>> {
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/invalid_project")
        .arg("--no-cache")
        .arg("for-file")
        .arg("ruby/app/services/multi_owned.rb")
        .assert()
        .failure()
        .stdout(predicate::eq(indoc! {"
            Error: file is owned by multiple teams!
            
            Team: Payments
            Github Team: @PaymentTeam
            Team YML: config/teams/payments.yml
            Description:
            - Owner annotation at the top of the file
            
            Team: Payroll
            Github Team: @PayrollTeam
            Team YML: config/teams/payroll.yml
            Description:
            - Owner specified in `ruby/app/services/.codeowner`
        "}));
    Ok(())
}
