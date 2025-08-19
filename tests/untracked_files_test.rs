use assert_cmd::prelude::*;
use std::{error::Error, path::Path, process::Command};

mod common;
use common::setup_fixture_repo;

use crate::common::{git_add_all_files, git_reset_all};

const FIXTURE: &str = "tests/fixtures/invalid_project";

#[test]
fn test_skip_untracked_files() -> Result<(), Box<dyn Error>> {
    // Arrange: copy fixture to temp dir and change a single CODEOWNERS mapping
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    // Act + Assert
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .assert()
        .failure();

    // should succeed if all files are untracked
    git_reset_all(project_root);
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .assert()
        .success();

    Ok(())
}
