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
            "}),
    )?;
    Ok(())
}

#[test]
fn test_for_file_json() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "invalid_project",
        &["for-file", "ruby/app/models/blockchain.rb", "--json"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {r#"
            {
              "team_name": "Unowned",
              "github_team": "Unowned",
              "team_yml": "",
              "description": []
            }
            "#}),
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

#[test]
fn test_for_file_multiple_owners_json() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "invalid_project",
        &["for-file", "ruby/app/services/multi_owned.rb", "--json"],
        false,
        OutputStream::Stdout,
        predicate::eq(indoc! {r#"
            {
              "validation_errors": [
                "Error: file is owned by multiple teams!",
                "\nTeam: Payments\nGithub Team: @PaymentTeam\nTeam YML: config/teams/payments.yml\nDescription:\n- Owner annotation at the top of the file",
                "\nTeam: Payroll\nGithub Team: @PayrollTeam\nTeam YML: config/teams/payroll.yml\nDescription:\n- Owner specified in `ruby/app/services/.codeowner`"
              ]
            }
            "#}),
    )?;
    Ok(())
}

#[test]
fn test_for_file_nonexistent_json() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "invalid_project",
        &["for-file", "nonexistent/file.rb", "--json"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {r#"
            {
              "team_name": "Unowned",
              "github_team": "Unowned",
              "team_yml": "",
              "description": []
            }
            "#}),
    )?;
    Ok(())
}
