use assert_cmd::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};

mod common;
use common::{git_add_all_files, setup_fixture_repo};

const FIXTURE: &str = "tests/fixtures/valid_project";

/// rubyatscale/code_ownership#149: adding a new team config must not require
/// `git add`-ing it first. The project walk excludes untracked files by
/// design (see tests/untracked_source_file_test.rs and codeowners-rs#46/#74/
/// #76 - it lets a developer keep scratch files around without being forced
/// to assign them an owner), but that exclusion used to apply to team config
/// files too - so a brand-new, not-yet-staged team file was invisible to the
/// walk, and any *already-tracked* file that should newly become owned by
/// that team was reported as unowned instead (the team file disappears from
/// the walk, but the file it's supposed to own is still there and still
/// expected to have an owner - simply making both files new/untracked
/// together hides the bug, since then neither is visible and no mismatch is
/// ever observed).
///
/// This mirrors the issue's exact repro shape: README.md already exists and
/// is already committed (as it would be from `rails new` or any prior
/// commit); only the *new team* and the *config edit adding README.md to
/// owned_globs* are freshly created and not yet staged.
#[test]
fn test_validate_succeeds_after_adding_new_untracked_team_for_an_existing_tracked_file() -> Result<(), Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    // README.md is part of the initial commit, so it's already tracked
    // before the new team is ever introduced.
    fs::write(project_root.join("README.md"), "# hello\n")?;
    git_add_all_files(project_root);
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(project_root)
        .output()?;

    // A brand-new team config, added later and never staged.
    fs::write(
        project_root.join("config/teams/docs.yml"),
        "name: Docs\ngithub:\n  team: '@DocsTeam'\nowned_globs:\n  - README.md\n",
    )?;

    let config_path = project_root.join("config/code_ownership.yml");
    let config = fs::read_to_string(&config_path)?;
    let updated_config = config.replacen("owned_globs:\n", "owned_globs:\n  - \"README.md\"\n", 1);
    assert_ne!(
        config, updated_config,
        "expected to find and extend the owned_globs list in the fixture config"
    );
    fs::write(&config_path, updated_config)?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("generate-and-validate")
        .assert()
        .success();

    Ok(())
}
