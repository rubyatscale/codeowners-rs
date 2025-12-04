use assert_cmd::prelude::*;
use codeowners::runner::{self, RunConfig};
use predicates::prelude::*;
use std::fs;
use std::path::Path;
use std::{error::Error, process::Command};
use tempfile::TempDir;

#[allow(dead_code)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

#[allow(dead_code)]
pub fn run_codeowners<I, P>(
    relative_fixture_path: &str,
    command: &[&str],
    success: bool,
    stream: OutputStream,
    output_predicate: I,
) -> Result<(), Box<dyn Error>>
where
    I: assert_cmd::assert::IntoOutputPredicate<P>,
    P: Predicate<[u8]>,
{
    let fixture_root = Path::new("tests/fixtures").join(relative_fixture_path);
    let temp_dir = setup_fixture_repo(&fixture_root);
    let project_root = temp_dir.path();
    git_add_all_files(project_root);
    let mut cmd = Command::cargo_bin("codeowners")?;
    let assert = cmd.arg("--project-root").arg(project_root).arg("--no-cache").args(command).assert();
    let assert = if success { assert.success() } else { assert.failure() };
    match stream {
        OutputStream::Stdout => {
            assert.stdout(output_predicate);
        }
        OutputStream::Stderr => {
            assert.stderr(output_predicate);
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn teardown() {
    glob::glob("tests/fixtures/*/tmp/cache/codeowners")
        .expect("Failed to read glob pattern")
        .filter_map(Result::ok)
        .for_each(|cache_dir| {
            if let Err(err) = fs::remove_dir_all(&cache_dir) {
                eprintln!("Failed to remove {} during test teardown: {}", &cache_dir.display(), err);
            }
        });
}

#[allow(dead_code)]
pub fn copy_dir_recursive(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("failed to create destination root");
    for entry in fs::read_dir(from).expect("failed to read source dir") {
        let entry = entry.expect("failed to read dir entry");
        let file_type = entry.file_type().expect("failed to read file type");
        let src_path = entry.path();
        let dest_path = to.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dest_path);
        } else if file_type.is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).expect("failed to create parent dir");
            }
            fs::copy(&src_path, &dest_path).expect("failed to copy file");
        }
    }
}

#[allow(dead_code)]
pub fn git_reset_all(path: &Path) {
    let status = Command::new("git")
        .arg("reset")
        .current_dir(path)
        .output()
        .expect("failed to run git reset --all");
    assert!(
        status.status.success(),
        "git reset --all failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

#[allow(dead_code)]
pub fn git_add_all_files(path: &Path) {
    let status = Command::new("git")
        .arg("add")
        .arg("--all")
        .current_dir(path)
        .output()
        .expect("failed to run git add --all");
    assert!(
        status.status.success(),
        "git add --all failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
}

#[allow(dead_code)]
pub fn init_git_repo(path: &Path) {
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

#[allow(dead_code)]
pub fn is_file_staged(repo_root: &Path, rel_path: &str) -> bool {
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

#[allow(dead_code)]
pub fn build_run_config(project_root: &Path, codeowners_rel_path: &str) -> RunConfig {
    let project_root = project_root.canonicalize().expect("failed to canonicalize project root");
    let codeowners_file_path = project_root.join(codeowners_rel_path);
    let config_path = project_root.join("config/code_ownership.yml");
    RunConfig {
        project_root,
        codeowners_file_path,
        config_path,
        no_cache: true,
        executable_name: None,
    }
}

#[allow(dead_code)]
pub fn setup_fixture_repo(fixture_root: &Path) -> TempDir {
    let temp_dir = tempfile::tempdir().expect("failed to create tempdir");
    copy_dir_recursive(fixture_root, temp_dir.path());
    init_git_repo(temp_dir.path());
    temp_dir
}

#[allow(dead_code)]
pub fn assert_no_run_errors(result: &runner::RunResult) {
    assert!(result.io_errors.is_empty(), "io_errors: {:?}", result.io_errors);
    assert!(
        result.validation_errors.is_empty(),
        "validation_errors: {:?}",
        result.validation_errors
    );
}
