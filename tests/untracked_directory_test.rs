use assert_cmd::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};

mod common;
use common::{git_add_all_files, setup_fixture_repo};

const FIXTURE: &str = "tests/fixtures/valid_project";

fn generate(project_root: &Path) -> Result<String, Box<dyn Error>> {
    let codeowners_path = project_root.join("tmp/CODEOWNERS");
    fs::create_dir_all(codeowners_path.parent().unwrap())?;
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate")
        .arg("--skip-stage")
        .assert()
        .success();
    Ok(fs::read_to_string(&codeowners_path)?)
}

/// The walk doesn't enter a directory that holds no tracked file, so a vendored gem that exists only in one checkout doesn't
/// change the CODEOWNERS file that checkout generates.
#[test]
fn test_generate_leaves_out_a_vendored_gem_with_no_tracked_files() -> Result<(), Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);
    let untrack = Command::new("git")
        .args(["rm", "-r", "-q", "--cached", "gems/pets"])
        .current_dir(project_root)
        .output()?;
    assert!(untrack.status.success(), "{}", String::from_utf8_lossy(&untrack.stderr));

    let codeowners = generate(project_root)?;
    assert!(codeowners.contains("/gems/payroll_calculator/**/** @PayrollTeam"), "{codeowners}");
    assert!(!codeowners.contains("/gems/pets/"), "{codeowners}");
    Ok(())
}

/// `git ls-files` lists a submodule as one path, without its contents, so a vendored gem that's a submodule is tracked too.
#[test]
fn test_generate_keeps_a_vendored_gem_that_is_a_submodule() -> Result<(), Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);
    for args in [
        vec!["rm", "-r", "-q", "--cached", "gems/pets"],
        vec![
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,0123456789abcdef0123456789abcdef01234567,gems/pets",
        ],
    ] {
        let output = Command::new("git").args(&args).current_dir(project_root).output()?;
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    }

    let codeowners = generate(project_root)?;
    assert!(codeowners.contains("/gems/pets/**/** @UX"), "{codeowners}");
    Ok(())
}

/// Team files count before they're tracked (rubyatscale/code_ownership#149), so the walk still enters an untracked directory
/// that the team file glob can match in.
#[test]
fn test_generate_reads_an_untracked_team_file_in_a_new_directory() -> Result<(), Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);
    fs::create_dir_all(project_root.join("config/teams/docs"))?;
    fs::write(
        project_root.join("config/teams/docs/docs.yml"),
        "name: Docs\ngithub:\n  team: '@DocsTeam'\nowned_globs:\n  - ruby/app/models/bank_account.rb\n",
    )?;

    let codeowners = generate(project_root)?;
    assert!(codeowners.contains("/config/teams/docs/docs.yml @DocsTeam"), "{codeowners}");
    assert!(codeowners.contains("/ruby/app/models/bank_account.rb @DocsTeam"), "{codeowners}");
    Ok(())
}
