use indoc::indoc;
use predicates::prelude::*;
use std::error::Error;

mod common;
use common::OutputStream;
use common::run_codeowners;

#[test]
fn test_validate() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "invalid_project",
        &["validate"],
        false,
        OutputStream::Stdout,
        predicate::eq(indoc! {"

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
    - ruby/app/unowned.rb

    "}),
    )?;
    Ok(())
}

#[test]
fn test_for_file() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "invalid_project",
        &["for-file", "ruby/app/models/blockchain.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {"
            Team: Unowned
            Github Team: Unowned
            Team YML: 
            Description:
            - Unowned
            "}),
    )?;
    Ok(())
}

#[test]
fn test_for_file_multiple_owners() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "invalid_project",
        &["for-file", "ruby/app/services/multi_owned.rb"],
        false,
        OutputStream::Stdout,
        predicate::eq(indoc! {"
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
        "}),
    )?;
    Ok(())
}
