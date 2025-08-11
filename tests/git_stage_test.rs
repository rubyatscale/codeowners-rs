use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use codeowners::runner::{self, RunConfig};

fn copy_dir_recursive(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("failed to create destination root");
    for entry in fs::read_dir(from).expect("failed to read source dir") {
        let entry = entry.expect("failed to read dir entry");
        let file_type = entry.file_type().expect("failed to read file type");
        let src_path = entry.path();
        let dest_path = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dest_path);
        } else if file_type.is_file() {
            // Ensure parent exists then copy
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).expect("failed to create parent dir");
            }
            fs::copy(&src_path, &dest_path).expect("failed to copy file");
        }
    }
}

fn init_git_repo(path: &Path) {
    // Initialize a new git repository in the temp project
    let status = Command::new("git")
        .arg("init")
        .current_dir(path)
        .output()
        .expect("failed to run git init");
    assert!(
        status.status.success(),
        "git init failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );

    // Configure a dummy identity to appease git if commits ever happen; not strictly needed for staging
    let _ = Command::new("git")
        .arg("config")
        .arg("user.email")
        .arg("test@example.com")
        .current_dir(path)
        .output();
    let _ = Command::new("git")
        .arg("config")
        .arg("user.name")
        .arg("Test User")
        .current_dir(path)
        .output();
}

fn is_file_staged(repo_root: &Path, rel_path: &str) -> bool {
    // Use git diff --name-only --cached to list staged files
    let output = Command::new("git")
        .arg("diff")
        .arg("--name-only")
        .arg("--cached")
        .current_dir(repo_root)
        .output()
        .expect("failed to run git diff --cached");
    assert!(
        output.status.success(),
        "git diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().any(|line| line.trim() == rel_path)
}

fn build_run_config(project_root: &Path, codeowners_rel_path: &str) -> RunConfig {
    let project_root = project_root.canonicalize().expect("failed to canonicalize project root");
    let codeowners_file_path = project_root.join(codeowners_rel_path);
    let config_path = project_root.join("config/code_ownership.yml");
    RunConfig {
        project_root,
        codeowners_file_path,
        config_path,
        no_cache: true,
    }
}

#[test]
fn test_generate_stages_codeowners() {
    let temp_dir = tempfile::tempdir().expect("failed to create tempdir");
    let temp_root = temp_dir.path().to_path_buf();

    // Copy the valid project fixture into a temporary git repo
    let fixture_root = PathBuf::from("tests/fixtures/valid_project");
    copy_dir_recursive(&fixture_root, &temp_root);
    init_git_repo(&temp_root);

    // Run generate with staging enabled, targeting the standard CODEOWNERS path
    let run_config = build_run_config(&temp_root, ".github/CODEOWNERS");
    let result = runner::generate(&run_config, true);
    assert!(result.io_errors.is_empty(), "io_errors: {:?}", result.io_errors);
    assert!(
        result.validation_errors.is_empty(),
        "validation_errors: {:?}",
        result.validation_errors
    );

    // Assert CODEOWNERS file exists and is staged
    let rel_path = ".github/CODEOWNERS";
    assert!(run_config.codeowners_file_path.exists(), "CODEOWNERS file was not created");
    assert!(is_file_staged(&run_config.project_root, rel_path), "CODEOWNERS file was not staged");
}

#[test]
fn test_generate_and_validate_stages_codeowners() {
    let temp_dir = tempfile::tempdir().expect("failed to create tempdir");
    let temp_root = temp_dir.path().to_path_buf();

    // Copy the valid project fixture into a temporary git repo
    let fixture_root = PathBuf::from("tests/fixtures/valid_project");
    copy_dir_recursive(&fixture_root, &temp_root);
    init_git_repo(&temp_root);

    // Run generate_and_validate with staging enabled
    let run_config = build_run_config(&temp_root, ".github/CODEOWNERS");
    let result = runner::generate_and_validate(&run_config, vec![], true);
    assert!(result.io_errors.is_empty(), "io_errors: {:?}", result.io_errors);
    assert!(
        result.validation_errors.is_empty(),
        "validation_errors: {:?}",
        result.validation_errors
    );

    // Assert CODEOWNERS file exists and is staged
    let rel_path = ".github/CODEOWNERS";
    assert!(run_config.codeowners_file_path.exists(), "CODEOWNERS file was not created");
    assert!(is_file_staged(&run_config.project_root, rel_path), "CODEOWNERS file was not staged");
}
