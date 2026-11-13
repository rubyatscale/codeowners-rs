use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{error::Error, process::Command};

mod common;

use common::*;

// valid_project has to validate cleanly, so it has no unowned file. A path that doesn't exist is also reported as unowned, so
// naming one would test that instead.
fn valid_project_with_unowned_file() -> tempfile::TempDir {
    let temp_dir = setup_fixture_repo(std::path::Path::new("tests/fixtures/valid_project"));
    std::fs::write(temp_dir.path().join("ruby/app/unowned.rb"), "class Unowned; end\n").expect("failed to write unowned file");
    git_add_all_files(temp_dir.path());
    temp_dir
}

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
    let project = valid_project_with_unowned_file();

    codeowners(project.path(), &["validate", "ruby/app/unowned.rb"])?
        .failure()
        .stdout(predicate::eq("\nSome files are missing ownership\n- ruby/app/unowned.rb\n\n"));

    Ok(())
}

#[test]
fn test_validate_with_mixed_files() -> Result<(), Box<dyn Error>> {
    let project = valid_project_with_unowned_file();

    codeowners(project.path(), &["validate", "ruby/app/models/payroll.rb", "ruby/app/unowned.rb"])?
        .failure()
        .stdout(predicate::eq("\nSome files are missing ownership\n- ruby/app/unowned.rb\n\n"));

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
    let temp_dir = valid_project_with_unowned_file();
    let project_root = temp_dir.path();

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
        .stdout(predicate::eq("\nSome files are missing ownership\n- ruby/app/unowned.rb\n\n"));

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
fn test_validate_with_an_annotated_file() -> Result<(), Box<dyn Error>> {
    // ruby/app/models/bank_account.rb has a @team Payments annotation and is in CODEOWNERS.
    run_codeowners(
        "valid_project",
        &["validate", "ruby/app/models/bank_account.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    Ok(())
}

// A temp copy of valid_project with `change` made and staged, and CODEOWNERS left as it was.
fn valid_project_with(change: impl FnOnce(&std::path::Path) -> std::io::Result<()>) -> tempfile::TempDir {
    let temp_dir = setup_fixture_repo(std::path::Path::new("tests/fixtures/valid_project"));
    change(temp_dir.path()).expect("failed to change the fixture");
    git_add_all_files(temp_dir.path());
    temp_dir
}

#[test]
fn test_validate_with_files_reports_a_file_missing_from_codeowners() -> Result<(), Box<dyn Error>> {
    let project = valid_project_with(|root| std::fs::write(root.join("ruby/app/models/new_thing.rb"), "# @team Payroll\n"));

    codeowners(project.path(), &["validate", "ruby/app/models/new_thing.rb"])?
        .failure()
        .stdout(predicate::eq(
            "\nCODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file\n\
             - ruby/app/models/new_thing.rb is owned by Payroll, but CODEOWNERS doesn't list it\n\n",
        ));
    codeowners(project.path(), &["gv", "ruby/app/models/new_thing.rb"])?
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

#[test]
fn test_validate_with_files_reports_a_file_codeowners_gives_another_team() -> Result<(), Box<dyn Error>> {
    let project = valid_project_with(|root| {
        let file = root.join("ruby/app/models/bank_account.rb");
        std::fs::write(&file, std::fs::read_to_string(&file)?.replace("Payments", "Payroll"))
    });

    codeowners(project.path(), &["validate", "ruby/app/models/bank_account.rb"])?
        .failure()
        .stdout(predicate::str::contains(
            "- ruby/app/models/bank_account.rb is owned by Payroll, but CODEOWNERS gives it to Payments",
        ));

    Ok(())
}

#[test]
fn test_validate_with_files_accepts_a_team_left_out_of_codeowners() -> Result<(), Box<dyn Error>> {
    // A team with do_not_add_to_codeowners_file gets commented-out lines, which the CODEOWNERS lookup still reads.
    let project = valid_project_with(|root| {
        std::fs::write(
            root.join("config/teams/payments.yml"),
            "name: Payments\ngithub:\n  team: '@PaymentsTeam'\n  do_not_add_to_codeowners_file: true\n\
             owned_globs:\n  - ruby/app/payments/**/*\nunowned_globs:\n  - ruby/app/payments/foo/**/*\n",
        )
    });
    codeowners(project.path(), &["generate"])?.success();

    codeowners(
        project.path(),
        &["validate", "ruby/app/models/bank_account.rb", "ruby/app/payments/nacha.rb"],
    )?
    .success()
    .stdout(predicate::eq(""));

    Ok(())
}

#[test]
fn test_validate_files_respects_owned_globs_with_excluded_extensions() -> Result<(), Box<dyn Error>> {
    // Validates that files not matching owned_globs are silently skipped when
    // validate is called with an explicit file list.
    //
    // valid_project owned_globs: "{gems,config,javascript,ruby,components}/**/*.{rb,tsx,erb}"
    // .rbi files (Sorbet interface files) do NOT match this pattern and should be filtered.

    // Setup: Create a temporary copy of valid_project fixture
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();

    // Create .rbi files (Sorbet interface files) that do NOT match owned_globs
    // These files should be ignored by validate when specified in the file list
    let bank_account_rbi = project_root.join("ruby/app/models/bank_account.rbi");
    let payroll_rbi = project_root.join("ruby/app/models/payroll.rbi");

    std::fs::write(
        &bank_account_rbi,
        "# typed: strict\n# RBI file for BankAccount\nclass BankAccount; end\n",
    )?;
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

    // Step 2: Run validate with BOTH .rb and .rbi files in the list.
    // .rbi files should be silently filtered; only .rb files validated; command succeeds.
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        // Mix .rb and .rbi files in the argument list
        .arg("ruby/app/models/bank_account.rb") // Should be validated (matches owned_globs)
        .arg("ruby/app/models/bank_account.rbi") // Should be SKIPPED (doesn't match)
        .arg("ruby/app/models/payroll.rb") // Should be validated (matches owned_globs)
        .arg("ruby/app/models/payroll.rbi") // Should be SKIPPED (doesn't match)
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

// ============================================================================
// GLOB FILTERING TESTS: Verify validate with files respects owned_globs
// ============================================================================
//
// These tests ensure that when validate is called with explicit file paths,
// it correctly filters files based on owned_globs configuration. Files that
// don't match owned_globs should be silently skipped, not reported as unowned.

#[test]
fn test_validate_filters_multiple_non_matching_extensions() -> Result<(), Box<dyn Error>> {
    // Test that various file types not in owned_globs are filtered out
    // valid_project owned_globs: "{gems,config,javascript,ruby,components}/**/*.{rb,tsx,erb}"
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();

    // Create files with extensions NOT in owned_globs
    std::fs::write(project_root.join("ruby/app/models/test.rbi"), "# Sorbet RBI file")?;
    std::fs::write(project_root.join("ruby/app/models/test.md"), "# Markdown doc")?;
    std::fs::write(project_root.join("ruby/app/models/test.txt"), "Plain text")?;
    std::fs::write(project_root.join("ruby/app/models/test.json"), "{}")?;

    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    // Generate CODEOWNERS (will only include .rb, .tsx, .erb files)
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();

    // Validate with a mix of matching and non-matching files
    // All non-matching should be filtered, matching ones should succeed
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/app/models/payroll.rb") // matches owned_globs, is owned
        .arg("ruby/app/models/test.rbi") // doesn't match owned_globs
        .arg("ruby/app/models/test.md") // doesn't match owned_globs
        .arg("ruby/app/models/test.txt") // doesn't match owned_globs
        .arg("ruby/app/models/test.json") // doesn't match owned_globs
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

#[test]
fn test_validate_filters_files_outside_owned_directories() -> Result<(), Box<dyn Error>> {
    // Test that files in directories not matching owned_globs are filtered
    // valid_project owned_globs: "{gems,config,javascript,ruby,components}/**/*.{rb,tsx,erb}"
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();

    // Create .rb files OUTSIDE the owned directories
    std::fs::create_dir_all(project_root.join("scripts"))?;
    std::fs::write(project_root.join("scripts/deploy.rb"), "# Deploy script")?;
    std::fs::create_dir_all(project_root.join("bin"))?;
    std::fs::write(project_root.join("bin/run.rb"), "# Run script")?;

    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    // Generate CODEOWNERS
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();

    // Validate with files both inside and outside owned directories
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/app/models/payroll.rb") // inside ruby/, matches owned_globs
        .arg("scripts/deploy.rb") // outside owned dirs, filtered
        .arg("bin/run.rb") // outside owned dirs, filtered
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

#[test]
fn test_validate_respects_unowned_globs() -> Result<(), Box<dyn Error>> {
    // Test that files matching unowned_globs are filtered out even if they match owned_globs
    let fixture_root = std::path::Path::new("tests/fixtures/valid_project");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();

    // Read and modify the config to add unowned_globs
    let config_path = project_root.join("config/code_ownership.yml");
    let config_content = std::fs::read_to_string(&config_path)?;
    let updated_config = config_content.replace("unowned_globs:", "unowned_globs:\n  - ruby/app/models/ignored_*.rb");
    std::fs::write(&config_path, updated_config)?;

    // Create a file that matches owned_globs but also matches unowned_globs
    std::fs::write(
        project_root.join("ruby/app/models/ignored_test.rb"),
        "# This file should be ignored via unowned_globs",
    )?;

    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    // Generate CODEOWNERS (ignored_test.rb should NOT be included)
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();

    // Verify the ignored file is NOT in CODEOWNERS
    let codeowners_content = std::fs::read_to_string(&codeowners_path)?;
    assert!(
        !codeowners_content.contains("ignored_test.rb"),
        "ignored_test.rb should not be in CODEOWNERS"
    );

    // Validate with the ignored file - should be filtered by unowned_globs
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/app/models/payroll.rb") // owned, should validate
        .arg("ruby/app/models/ignored_test.rb") // matches unowned_globs, should be filtered
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}
