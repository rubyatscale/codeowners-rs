use assert_cmd::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};

mod common;
use common::{git_add_all_files, setup_fixture_repo};

const FIXTURE: &str = "tests/fixtures/valid_project";

/// A directory can be force-tracked in git even when it matches a
/// gitignore-style rule that lives outside the tracked tree (a personal
/// global `core.excludesFile`, or a local, uncommitted `.git/info/exclude`
/// entry). `generate` must still emit ownership for such directories:
/// inclusion is decided by `tracked_files` (git ls-files), so also honoring
/// git's own ignore semantics at the directory-walk level can silently prune
/// tracked, owned files depending on the machine's git config alone --
/// nothing committed to the repo. `.git/info/exclude` is used here as a
/// reliable stand-in for a developer's global gitignore, which is what
/// originally surfaced this bug (see PR discussion).
#[test]
fn test_generate_ignores_local_exclude_rules_for_tracked_directories() -> Result<(), Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let exclude_path = project_root.join(".git/info/exclude");
    let mut existing = fs::read_to_string(&exclude_path).unwrap_or_default();
    existing.push_str("\nruby/app/payroll/\n");
    fs::write(&exclude_path, existing)?;

    let codeowners_path = project_root.join("tmp/CODEOWNERS");
    fs::create_dir_all(codeowners_path.parent().unwrap())?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();

    let actual_codeowners = fs::read_to_string(&codeowners_path)?;
    assert!(
        actual_codeowners.contains("/ruby/app/payroll/**/** @PayrollTeam"),
        "expected ruby/app/payroll ownership to survive a local .git/info/exclude rule matching it, got:\n{actual_codeowners}"
    );

    Ok(())
}
