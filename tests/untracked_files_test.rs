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

    let skip_untracked_files_config_path = project_root.join(".codeowners.yml");
    fs::write(&skip_untracked_files_config_path, "skip_untracked_files: true")?;

    // should succeed if skip_untracked_false is false
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .assert()
        .success();

    // should fail if skip_untracked_false is false
    fs::write(&skip_untracked_files_config_path, "skip_untracked_files: false")?;
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .assert()
        .failure();

    Ok(())
}
