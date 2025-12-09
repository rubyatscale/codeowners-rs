use assert_cmd::prelude::*;
use std::{error::Error, fs, process::Command};

mod common;

use common::*;

/// BUG 1: validate_files only checks CODEOWNERS, not derived ownership
/// 
/// When validate() is called with file_paths, it only checks the CODEOWNERS file
/// via team_for_file_from_codeowners(). This means it doesn't check:
/// - File annotations (@team)
/// - Invalid team names
/// - Directory ownership
/// - Package ownership
/// - Team globs
/// 
/// Compare with validate_all() which calls ownership.validate() which runs:
/// - validate_invalid_team() - checks all team references are valid
/// - validate_file_ownership() - checks all files have exactly one owner
/// - validate_codeowners_file() - checks CODEOWNERS is up to date
#[test]
fn bug_1_validate_files_misses_invalid_team_annotation() -> Result<(), Box<dyn Error>> {
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    // Create a file with an invalid team annotation
    let file_with_bad_team = project_root.join("ruby/app/models/bad_team.rb");
    fs::write(
        &file_with_bad_team,
        "# @team NonExistentTeam\nclass BadTeam\nend\n",
    )?;

    git_add_all_files(project_root);

    // Generate CODEOWNERS (this file won't be in CODEOWNERS because team doesn't exist)
    let codeowners_path = project_root.join("tmp/CODEOWNERS");
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();

    // validate with no files - SHOULD report invalid team
    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .output()?;
    
    println!("validate (no files) output: {}", String::from_utf8_lossy(&output.stdout));
    println!("validate (no files) stderr: {}", String::from_utf8_lossy(&output.stderr));
    
    assert!(
        !output.status.success(),
        "BUG: validate() without files SHOULD fail with invalid team"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("NonExistentTeam") || stdout.contains("invalid team"),
        "BUG: validate() without files SHOULD report invalid team"
    );

    // validate with the specific file - SHOULD ALSO report invalid team, but DOESN'T
    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/app/models/bad_team.rb")
        .output()?;

    println!("validate (with file) output: {}", String::from_utf8_lossy(&output.stdout));
    println!("validate (with file) stderr: {}", String::from_utf8_lossy(&output.stderr));

    // THIS IS THE BUG: validate with files only checks CODEOWNERS, not team validity
    // It reports the file as "unowned" instead of "invalid team"
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("Unowned") {
        println!("❌ BUG CONFIRMED: validate_files reports 'Unowned' instead of 'invalid team'");
    }
    
    Ok(())
}

/// BUG 2: validate_files and generate_and_validate don't check for stale team references
/// 
/// When a team is renamed/deleted in config/teams, files that reference the old team
/// should be flagged as having invalid team references. But validate_files only checks
/// CODEOWNERS, which gets regenerated with the current teams.
#[test]
fn bug_2_validate_files_misses_stale_team_after_rename() -> Result<(), Box<dyn Error>> {
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    // Create a file owned by Payments team (which exists)
    let test_file = project_root.join("ruby/app/models/payment_test.rb");
    fs::write(&test_file, "# @team Payments\nclass PaymentTest\nend\n")?;

    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    // Initial generate-and-validate should succeed
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate-and-validate")
        .arg("ruby/app/models/payment_test.rb")
        .assert()
        .success();

    // Now rename the Payments team to PaymentsNew in the team config
    let payments_team_file = project_root.join("config/teams/payments.yml");
    let team_content = fs::read_to_string(&payments_team_file)?;
    let new_content = team_content.replace("name: Payments", "name: PaymentsNew");
    fs::write(&payments_team_file, new_content)?;

    // Full validate (no files) SHOULD fail - file references old team name
    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .output()?;

    println!("validate (no files, after rename) output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(
        !output.status.success(),
        "BUG: validate() without files SHOULD fail after team rename"
    );

    // But generate-and-validate with the file MIGHT NOT report the issue correctly
    // Because it generates a new CODEOWNERS (file won't be in it), then validates (file is unowned)
    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate-and-validate")
        .arg("ruby/app/models/payment_test.rb")
        .output()?;

    println!("generate-and-validate (after rename) output: {}", String::from_utf8_lossy(&output.stdout));
    println!("generate-and-validate (after rename) stderr: {}", String::from_utf8_lossy(&output.stderr));

    // The file should be reported as having an "invalid team" but instead
    // it's reported as "unowned" because validate_files only checks CODEOWNERS
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.contains("Unowned") && !stdout.contains("invalid") {
        println!("❌ BUG CONFIRMED: generate-and-validate reports 'Unowned' instead of 'invalid team: Payments'");
    }

    Ok(())
}

/// BUG 3: validate_files doesn't check for multiple owners
/// 
/// When a file has multiple ownership sources (e.g., both @team annotation and package ownership),
/// validate_all() reports this as an error. But validate_files() doesn't check for this.
#[test]
fn bug_3_validate_files_misses_multiple_owners() -> Result<(), Box<dyn Error>> {
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    // Find a package that has an owner
    let package_yml = project_root.join("ruby/gems/payroll/package.yml");
    let package_content = "enforce_privacy: false\nowner: Payroll\n";
    fs::create_dir_all(package_yml.parent().unwrap())?;
    fs::write(&package_yml, package_content)?;

    // Create a file in that package with a DIFFERENT team annotation
    let test_file = project_root.join("ruby/gems/payroll/lib/payroll.rb");
    fs::create_dir_all(test_file.parent().unwrap())?;
    fs::write(&test_file, "# @team Payments\nclass Payroll\nend\n")?;

    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    // Full validate (no files) SHOULD fail - file has multiple owners
    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .output()?;

    println!("validate (no files) output: {}", String::from_utf8_lossy(&output.stdout));
    
    let stdout_no_files = String::from_utf8_lossy(&output.stdout);
    let has_multiple_owners_error = stdout_no_files.contains("multiple") || stdout_no_files.contains("more than one");
    
    if !output.status.success() && has_multiple_owners_error {
        println!("✓ validate() without files correctly reports multiple owners");
    }

    // validate with the file - SHOULD also report multiple owners
    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/gems/payroll/lib/payroll.rb")
        .output()?;

    println!("validate (with file) output: {}", String::from_utf8_lossy(&output.stdout));

    // BUG: validate_files doesn't check for multiple owners
    // It will just check if the file is in CODEOWNERS (it might be, with one of the owners)
    let stdout_with_file = String::from_utf8_lossy(&output.stdout);
    if !stdout_with_file.contains("multiple") && !stdout_with_file.contains("more than one") {
        println!("❌ BUG CONFIRMED: validate_files doesn't detect multiple owners");
    }

    Ok(())
}

/// BUG 4: validate_files doesn't detect stale CODEOWNERS file
/// 
/// validate_all() checks if the CODEOWNERS file is stale (doesn't match what would be generated).
/// But validate_files() doesn't perform this check.
#[test]
fn bug_4_validate_files_misses_stale_codeowners() -> Result<(), Box<dyn Error>> {
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    // Generate initial CODEOWNERS
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();

    // Now manually modify CODEOWNERS to make it stale
    let mut codeowners_content = fs::read_to_string(&codeowners_path)?;
    codeowners_content.push_str("\n# Manually added stale content\n/fake/path @fake-team\n");
    fs::write(&codeowners_path, codeowners_content)?;

    // Full validate (no files) SHOULD fail - CODEOWNERS is stale
    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .output()?;

    println!("validate (no files) output: {}", String::from_utf8_lossy(&output.stdout));
    
    assert!(
        !output.status.success(),
        "BUG: validate() without files SHOULD fail when CODEOWNERS is stale"
    );

    // validate with files - SHOULD also fail because CODEOWNERS is stale
    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/app/models/bank_account.rb")
        .output()?;

    println!("validate (with files) output: {}", String::from_utf8_lossy(&output.stdout));

    // BUG: validate_files doesn't check if CODEOWNERS is stale
    if output.status.success() {
        println!("❌ BUG CONFIRMED: validate_files doesn't detect stale CODEOWNERS");
    }

    Ok(())
}

/// BUG 5: Inconsistent behavior between validate and generate-and-validate
/// 
/// generate-and-validate generates CODEOWNERS then validates. But the validate step
/// uses validate_files which has different behavior than validate_all.
#[test]
fn bug_5_generate_and_validate_inconsistent_with_validate() -> Result<(), Box<dyn Error>> {
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    // Create a file with invalid team
    let bad_file = project_root.join("ruby/app/models/invalid.rb");
    fs::write(&bad_file, "# @team InvalidTeam\nclass Invalid\nend\n")?;

    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    // Run validate (no files) - should detect invalid team
    let validate_all_output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .output()?;

    println!("validate (all files) output: {}", String::from_utf8_lossy(&validate_all_output.stdout));

    // Run generate-and-validate with the specific file
    let gen_val_output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate-and-validate")
        .arg("ruby/app/models/invalid.rb")
        .output()?;

    println!("generate-and-validate output: {}", String::from_utf8_lossy(&gen_val_output.stdout));

    // Compare the outputs - they should be similar for the same error
    let validate_all_stdout = String::from_utf8_lossy(&validate_all_output.stdout);
    let gen_val_stdout = String::from_utf8_lossy(&gen_val_output.stdout);

    let validate_all_mentions_invalid = validate_all_stdout.contains("invalid") || validate_all_stdout.contains("InvalidTeam");
    let gen_val_mentions_invalid = gen_val_stdout.contains("invalid") || gen_val_stdout.contains("InvalidTeam");

    if validate_all_mentions_invalid && !gen_val_mentions_invalid {
        println!("❌ BUG CONFIRMED: generate-and-validate gives different error than validate for same issue");
        println!("  validate says: {}", validate_all_stdout);
        println!("  generate-and-validate says: {}", gen_val_stdout);
    }

    Ok(())
}

