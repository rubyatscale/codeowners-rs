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
        .arg("--no-cache")
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
        .arg("--no-cache")
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
        .arg("--no-cache")
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

#[test]
fn test_validate_files_respects_owned_globs_with_excluded_extensions() -> Result<(), Box<dyn Error>> {
    // ============================================================================
    // THIS TEST CURRENTLY FAILS ON MAIN - IT DEMONSTRATES THE BUG
    // ============================================================================
    //
    // BUG DESCRIPTION:
    // When validate is called with a file list, it validates ALL provided files
    // without checking if they match owned_globs configuration.
    //
    // CONFIGURATION:
    // valid_project has: owned_globs = "**/*.{rb,tsx,erb}"
    // Notice: .rbi files (Sorbet interface files) are NOT in this pattern
    //
    // EXPECTED BEHAVIOR:
    // - .rbi files should be SILENTLY SKIPPED (don't match owned_globs)
    // - Only .rb files should be validated against CODEOWNERS
    // - Command should SUCCEED because all validated files are owned
    //
    // ACTUAL BEHAVIOR (BUG):
    // - ALL files are validated (including .rbi files)
    // - .rbi files are not in CODEOWNERS (correctly excluded during generate)
    // - .rbi files are reported as "Unowned"
    // - Command FAILS with validation errors
    //
    // ROOT CAUSE:
    // src/runner.rs lines 112-143: validate_files() iterates all file_paths
    // without applying the owned_globs/unowned_globs filter that
    // project_builder.rs:172 uses when no files are specified
    //
    // FIX NEEDED:
    // Filter file_paths by owned_globs and unowned_globs before validation
    // ============================================================================

    // Setup: Create a temporary copy of valid_project fixture
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();

    // Create .rbi files (Sorbet interface files) that do NOT match owned_globs
    // These files should be ignored by validate when specified in the file list
    let bank_account_rbi = project_root.join("ruby/app/models/bank_account.rbi");
    let payroll_rbi = project_root.join("ruby/app/models/payroll.rbi");

    std::fs::write(&bank_account_rbi, "# typed: strict\n# RBI file for BankAccount\nclass BankAccount; end\n")?;
    std::fs::write(&payroll_rbi, "# typed: strict\n# RBI file for Payroll\nclass Payroll; end\n")?;

    git_add_all_files(project_root);

    // Step 1: Generate CODEOWNERS
    // This should ONLY include .rb files (not .rbi) because .rbi doesn't match owned_globs
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

    // Verify: CODEOWNERS contains .rb files but NOT .rbi files
    let codeowners_content = std::fs::read_to_string(&codeowners_path)?;
    assert!(
        codeowners_content.contains("bank_account.rb"),
        "CODEOWNERS should contain .rb files (they match owned_globs)"
    );
    assert!(
        !codeowners_content.contains("bank_account.rbi"),
        "CODEOWNERS should NOT contain .rbi files (they don't match owned_globs)"
    );

    // Step 2: Run validate with BOTH .rb and .rbi files in the list
    // EXPECTED: .rbi files are silently skipped, only .rb files validated, succeeds
    // ACTUAL (BUG): All files validated, .rbi reported as unowned, command fails
    //
    // ============================================================================
    // THIS ASSERTION WILL FAIL ON MAIN (proving the bug exists)
    // ============================================================================
    //
    // The command should succeed because:
    // 1. .rbi files should be filtered out (don't match owned_globs)
    // 2. Only .rb files should be validated
    // 3. All .rb files are properly owned in CODEOWNERS
    //
    // But it currently fails because:
    // 1. ALL files (including .rbi) are validated
    // 2. .rbi files are not in CODEOWNERS
    // 3. Validation error: "Unowned files detected: ruby/app/models/bank_account.rbi ..."
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        // Mix .rb and .rbi files in the argument list
        .arg("ruby/app/models/bank_account.rb")   // Should be validated (matches owned_globs)
        .arg("ruby/app/models/bank_account.rbi")  // Should be SKIPPED (doesn't match)
        .arg("ruby/app/models/payroll.rb")        // Should be validated (matches owned_globs)
        .arg("ruby/app/models/payroll.rbi")       // Should be SKIPPED (doesn't match)
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}
