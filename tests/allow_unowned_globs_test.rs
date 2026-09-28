use assert_cmd::prelude::*;
use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};

mod common;
use common::{OutputStream, git_add_all_files, run_codeowners, setup_fixture_repo};

const FIXTURE: &str = "tests/fixtures/allow_unowned_globs";

#[test]
fn test_validate_passes_with_unowned_files_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_codeowners("allow_unowned_globs", &["validate"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_generate_and_validate_passes_with_unowned_files_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_codeowners("allow_unowned_globs", &["gv"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_validate_files_passes_for_an_unowned_file_in_an_allowed_glob() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "allow_unowned_globs",
        &["validate", "app/deprecated/old.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;
    Ok(())
}

#[test]
fn test_annotations_still_assign_owners_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "allow_unowned_globs",
        &["for-file", "app/deprecated/annotated_old.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(indoc! {"
            Team: Foo
            Github Team: @FooTeam
            Team YML: config/teams/foo.yml
            Description:
            - Owner annotation at the top of the file
        "}),
    )?;
    Ok(())
}

#[test]
fn test_unowned_file_in_an_allowed_glob_is_reported_as_unowned() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        "allow_unowned_globs",
        &["for-file", "app/deprecated/old.rb"],
        true,
        OutputStream::Stdout,
        predicate::str::contains("Team: Unowned"),
    )?;
    Ok(())
}

fn run_on_modified_fixture(
    modify: impl FnOnce(&Path) -> std::io::Result<()>,
    args: &[&str],
) -> Result<assert_cmd::assert::Assert, Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let project_root = temp_dir.path();
    modify(project_root)?;
    git_add_all_files(project_root);
    let assert = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .args(args)
        .assert();
    Ok(assert)
}

fn add_unowned_file_outside_allowed_globs(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("app/stray.rb"), "puts 'no owner'\n")
}

#[test]
fn test_validate_reports_unowned_files_outside_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(add_unowned_file_outside_allowed_globs, &["validate"])?
        .failure()
        .stdout(predicate::str::contains("Some files are missing ownership"))
        .stdout(predicate::str::contains("- app/stray.rb"))
        .stdout(predicate::str::contains("app/deprecated/old.rb").not());
    Ok(())
}

#[test]
fn test_validate_files_reports_an_unowned_file_outside_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(add_unowned_file_outside_allowed_globs, &["validate", "app/stray.rb"])?
        .failure()
        .stdout(predicate::str::contains("Unowned files detected:"))
        .stdout(predicate::str::contains("app/stray.rb"));
    Ok(())
}

#[test]
fn test_validate_reports_unowned_files_without_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(
        |project_root| {
            fs::write(
                project_root.join("config/code_ownership.yml"),
                "---\nowned_globs:\n  - \"{app,config}/**/*.rb\"\n",
            )
        },
        &["validate"],
    )?
    .failure()
    .stdout(predicate::str::contains("Some files are missing ownership"))
    .stdout(predicate::str::contains("- app/deprecated/old.rb"));
    Ok(())
}

fn allow_every_unowned_file_with_one_at_the_project_root(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("root.rb"), "puts 'no owner'\n")?;
    fs::write(
        project_root.join("config/code_ownership.yml"),
        "---\nowned_globs:\n  - \"**/*.rb\"\nallow_unowned_globs:\n  - \"**/*\"\n",
    )
}

#[test]
fn test_double_star_glob_allows_unowned_files_at_the_project_root() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(allow_every_unowned_file_with_one_at_the_project_root, &["validate"])?
        .success()
        .stdout(predicate::eq(""));
    run_on_modified_fixture(allow_every_unowned_file_with_one_at_the_project_root, &["validate", "root.rb"])?
        .success()
        .stdout(predicate::eq(""));
    Ok(())
}
