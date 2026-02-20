use predicates::prelude::predicate;
use std::{error::Error, fs, path::Path};

mod common;

use common::OutputStream;
use common::git_add_all_files;
use common::run_codeowners;
use common::setup_fixture_repo;

#[test]
fn test_generate_uses_codeowners_path_from_config() -> Result<(), Box<dyn Error>> {
    let fixture_root = Path::new("tests/fixtures/custom_codeowners_path");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let mut cmd = assert_cmd::Command::cargo_bin("codeowners")?;
    cmd.arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();

    let expected_codeowners: String = std::fs::read_to_string(Path::new("tests/fixtures/custom_codeowners_path/expected/CODEOWNERS"))?;
    let actual_codeowners: String = std::fs::read_to_string(project_root.join("docs/CODEOWNERS"))?;

    assert_eq!(expected_codeowners, actual_codeowners);

    Ok(())
}

#[test]
fn test_cli_overrides_codeowners_path_from_config() -> Result<(), Box<dyn Error>> {
    fs::create_dir_all("tmp")?;
    let codeowners_abs = std::env::current_dir()?.join("tmp/CODEOWNERS");
    let codeowners_str = codeowners_abs.to_str().unwrap();

    run_codeowners(
        "custom_codeowners_path",
        &["--codeowners-file-path", codeowners_str, "generate"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    let expected_codeowners: String = std::fs::read_to_string(Path::new("tests/fixtures/custom_codeowners_path/expected/CODEOWNERS"))?;
    let actual_codeowners: String = std::fs::read_to_string(Path::new("tmp/CODEOWNERS"))?;

    assert_eq!(expected_codeowners, actual_codeowners);

    Ok(())
}

#[test]
fn test_validate_uses_codeowners_path_from_config() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "custom_codeowners_path",
        &["validate"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    Ok(())
}

#[test]
fn test_validate_uses_cli_override() -> Result<(), Box<dyn Error>> {
    fs::create_dir_all("tmp")?;
    let codeowners_abs = std::env::current_dir()?.join("tmp/CODEOWNERS");
    let codeowners_str = codeowners_abs.to_str().unwrap();

    run_codeowners(
        "custom_codeowners_path",
        &["--codeowners-file-path", codeowners_str, "generate"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    run_codeowners(
        "custom_codeowners_path",
        &["--codeowners-file-path", codeowners_str, "validate"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;

    Ok(())
}
