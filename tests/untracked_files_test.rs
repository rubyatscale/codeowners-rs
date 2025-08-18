use assert_cmd::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};

mod common;
use common::setup_fixture_repo;

const FIXTURE: &str = "tests/fixtures/invalid_project";

#[test]
fn test_skip_untracked_files() -> Result<(), Box<dyn Error>> {
    // Arrange: copy fixture to temp dir and change a single CODEOWNERS mapping
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();

    // Act + Assert
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .assert()
        .failure();

    // Add skip_untracked_false: false to project_root/config/code_ownership.yml
    let config_path = project_root.join("config/code_ownership.yml");
    let original = fs::read_to_string(&config_path)?;
    // Change payroll.rb ownership from @PayrollTeam to @PaymentsTeam to induce a mismatch
    let modified = original.replace("skip_untracked_files: false", "skip_untracked_files: true");
    fs::write(&config_path, modified)?;

    // should succeed if skip_untracked_false is false
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .assert()
        .success();

    Ok(())
}
