use assert_cmd::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};

mod common;
use common::{git_add_all_files, setup_fixture_repo};

const FIXTURE: &str = "tests/fixtures/valid_project";

/// The fix for rubyatscale/code_ownership#149 (see untracked_new_file_test.rs)
/// only exempts team config files from the tracked-files check - it must NOT
/// broaden inclusion to untracked source files in general. That exclusion is
/// intentional (codeowners-rs#46/#74/#76: "don't fail when untracked git
/// files aren't in codeowners"), letting a developer keep a scratch file
/// around locally without `validate` forcing them to assign it an owner.
#[test]
fn test_untracked_source_file_is_still_ignored() -> Result<(), Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(project_root)
        .output()?;

    // A new source file matching owned_globs, but never staged. If it were
    // included, this would fail with "missing ownership" since nothing
    // claims it; if it's correctly ignored (matching pre-#149-fix behavior
    // for non-team files), validation succeeds unchanged.
    fs::write(project_root.join("ruby/app/models/scratch.rb"), "class Scratch; end\n")?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("generate-and-validate")
        .assert()
        .success();

    Ok(())
}
