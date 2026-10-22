use assert_cmd::{assert::Assert, prelude::*};
use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, ffi::OsStr, fs, path::Path, process::Command};
use tempfile::TempDir;

mod common;
use common::{OutputStream, git_add_all_files, run_codeowners, setup_fixture_repo};

const FIXTURE: &str = "allow_unowned_globs";

type FixtureChange = fn(&Path) -> std::io::Result<()>;

#[test]
fn test_validate_passes_with_unowned_files_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_codeowners(FIXTURE, &["validate"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_generate_and_validate_passes_with_unowned_files_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_codeowners(FIXTURE, &["gv"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_validate_files_passes_for_an_unowned_file_in_an_allowed_glob() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        FIXTURE,
        &["validate", "app/deprecated/old.rb"],
        true,
        OutputStream::Stdout,
        predicate::eq(""),
    )?;
    Ok(())
}

#[test]
fn test_validate_files_passes_for_an_absolute_path_in_an_allowed_glob() -> Result<(), Box<dyn Error>> {
    validate_absolute_path(|_| Ok(()), "app/deprecated/old.rb")?
        .success()
        .stdout(predicate::eq(""));
    Ok(())
}

#[test]
fn test_annotations_still_assign_owners_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_codeowners(
        FIXTURE,
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
        FIXTURE,
        &["for-file", "app/deprecated/old.rb"],
        true,
        OutputStream::Stdout,
        predicate::str::contains("Team: Unowned"),
    )?;
    Ok(())
}

fn modified_fixture(modify: impl FnOnce(&Path) -> std::io::Result<()>) -> Result<TempDir, Box<dyn Error>> {
    let temp_dir = setup_fixture_repo(&Path::new("tests/fixtures").join(FIXTURE));
    modify(temp_dir.path())?;
    git_add_all_files(temp_dir.path());
    Ok(temp_dir)
}

fn codeowners(project_root: &Path, args: &[impl AsRef<OsStr>]) -> Result<Assert, Box<dyn Error>> {
    Ok(Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .args(args)
        .assert())
}

fn run_on_modified_fixture(modify: impl FnOnce(&Path) -> std::io::Result<()>, args: &[&str]) -> Result<Assert, Box<dyn Error>> {
    let temp_dir = modified_fixture(modify)?;
    codeowners(temp_dir.path(), args)
}

fn validate_absolute_path(modify: impl FnOnce(&Path) -> std::io::Result<()>, file: &str) -> Result<Assert, Box<dyn Error>> {
    let temp_dir = modified_fixture(modify)?;
    let file_absolute_path = temp_dir.path().join(file).canonicalize()?;
    codeowners(temp_dir.path(), &[OsStr::new("validate"), file_absolute_path.as_os_str()])
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

fn add_a_second_team_owning_the_allowed_glob(project_root: &Path) -> std::io::Result<()> {
    fs::write(
        project_root.join("config/teams/bar.yml"),
        "---\nname: Bar\ngithub:\n  team: '@BarTeam'\nowned_globs:\n  - app/deprecated/**/*\n",
    )
}

#[test]
fn test_validate_reports_multiple_owners_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(add_a_second_team_owning_the_allowed_glob, &["validate"])?
        .failure()
        .stdout(predicate::str::contains(
            "Code ownership should only be defined for each file in one way.",
        ))
        .stdout(predicate::str::contains("app/deprecated/annotated_old.rb\n owner:"));
    Ok(())
}

fn add_an_annotation_naming_an_unknown_team_in_an_allowed_glob(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("app/deprecated/typo.rb"), "# @team Typo\nputs 'typo'\n")
}

#[test]
fn test_validate_reports_invalid_teams_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(add_an_annotation_naming_an_unknown_team_in_an_allowed_glob, &["validate"])?
        .failure()
        .stdout(predicate::str::contains("Found invalid team references"))
        .stdout(predicate::str::contains(
            "- app/deprecated/typo.rb is referencing an invalid team - 'Typo'",
        ));
    Ok(())
}

#[test]
fn test_validate_files_reports_invalid_teams_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    // A list of files isn't checked for invalid teams. CODEOWNERS has no line for this file, and its annotation keeps
    // allow_unowned_globs from excusing it, so it's reported as unowned.
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_an_annotation_naming_an_unknown_team_in_an_allowed_glob,
            &[command, "app/deprecated/typo.rb"],
        )?
        .failure()
        .stdout(predicate::str::contains("Unowned files detected:"))
        .stdout(predicate::str::contains("app/deprecated/typo.rb"));
    }
    Ok(())
}

#[test]
fn test_validate_files_reports_invalid_teams_in_allowed_globs_for_an_absolute_path() -> Result<(), Box<dyn Error>> {
    validate_absolute_path(
        add_an_annotation_naming_an_unknown_team_in_an_allowed_glob,
        "app/deprecated/typo.rb",
    )?
    .failure()
    .stdout(predicate::str::contains("Unowned files detected:"))
    .stdout(predicate::str::contains("app/deprecated/typo.rb"));
    Ok(())
}

#[test]
fn test_validate_files_reports_invalid_teams_in_allowed_globs_for_a_path_with_dot_dot() -> Result<(), Box<dyn Error>> {
    // The owner lookup can't see the annotation without resolving the `..`, so the file mustn't be excused.
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_an_annotation_naming_an_unknown_team_in_an_allowed_glob,
            &[command, "app/deprecated/../deprecated/typo.rb"],
        )?
        .failure()
        .stdout(predicate::str::contains("Unowned files detected:"))
        .stdout(predicate::str::contains("app/deprecated/../deprecated/typo.rb"));
    }
    Ok(())
}

fn allow_every_file_and_annotate_one_with_an_unknown_team(project_root: &Path) -> std::io::Result<()> {
    add_an_annotation_naming_an_unknown_team_in_an_allowed_glob(project_root)?;
    fs::write(
        project_root.join("config/code_ownership.yml"),
        "---\nowned_globs:\n  - \"**/*.rb\"\nallow_unowned_globs:\n  - \"**/*\"\n",
    )
}

#[test]
fn test_validate_files_treats_a_dot_slash_path_like_a_plain_one() -> Result<(), Box<dyn Error>> {
    // Unlike the fixture's owned_globs, `**/*.rb` matches a path starting with `./`, so these reach the owner lookup.
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            allow_every_file_and_annotate_one_with_an_unknown_team,
            &[command, "./app/deprecated/typo.rb"],
        )?
        .failure()
        .stdout(predicate::str::contains("Unowned files detected:"))
        .stdout(predicate::str::contains("./app/deprecated/typo.rb"));
        run_on_modified_fixture(
            allow_every_file_and_annotate_one_with_an_unknown_team,
            &[command, "./app/deprecated/old.rb"],
        )?
        .success()
        .stdout(predicate::eq(""));
    }
    Ok(())
}

fn add_a_package_naming_an_unknown_team_in_an_allowed_glob(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("packs/typo/app"))?;
    fs::write(project_root.join("packs/typo/package.yml"), "owner: Typo\n")?;
    fs::write(project_root.join("packs/typo/app/thing.rb"), "puts 'typo'\n")?;
    fs::write(
        project_root.join("config/code_ownership.yml"),
        "---\nowned_globs:\n  - \"{app,config,packs}/**/*.rb\"\nallow_unowned_globs:\n  - \"app/deprecated/**/*\"\n  - \"packs/typo/**/*\"\n",
    )
}

fn add_a_javascript_package_naming_an_unknown_team_in_an_allowed_glob(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("frontend/typo/src"))?;
    fs::write(
        project_root.join("frontend/typo/package.json"),
        "{\"metadata\": {\"owner\": \"Typo\"}}\n",
    )?;
    fs::write(project_root.join("frontend/typo/src/thing.js"), "console.log('typo');\n")?;
    fs::write(
        project_root.join("config/code_ownership.yml"),
        "---\nowned_globs:\n  - \"{app,config,frontend}/**/*.{rb,js}\"\nallow_unowned_globs:\n  - \"app/deprecated/**/*\"\n  - \"frontend/typo/**/*\"\n",
    )
}

#[test]
fn test_validate_files_reports_a_package_naming_an_unknown_team_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    let packages: [(FixtureChange, &str); 2] = [
        (add_a_package_naming_an_unknown_team_in_an_allowed_glob, "packs/typo/app/thing.rb"),
        (
            add_a_javascript_package_naming_an_unknown_team_in_an_allowed_glob,
            "frontend/typo/src/thing.js",
        ),
    ];
    for (add_package, file) in packages {
        for command in ["validate", "gv"] {
            run_on_modified_fixture(add_package, &[command, file])?
                .failure()
                .stdout(predicate::str::contains("Unowned files detected:"))
                .stdout(predicate::str::contains(file));
        }
    }
    Ok(())
}

// Read as a glob, `packs/[typo]` would match `packs/t` rather than the package's own directory.
fn add_a_bracketed_package_naming_an_unknown_team_in_an_allowed_glob(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("packs/[typo]/app"))?;
    fs::write(project_root.join("packs/[typo]/package.yml"), "owner: Typo\n")?;
    fs::write(project_root.join("packs/[typo]/app/thing.rb"), "puts 'typo'\n")?;
    fs::create_dir_all(project_root.join("packs/t"))?;
    fs::write(project_root.join("packs/t/stray.rb"), "puts 'no owner'\n")?;
    fs::write(
        project_root.join("config/code_ownership.yml"),
        "---\nowned_globs:\n  - \"{app,config,packs}/**/*.rb\"\nallow_unowned_globs:\n  - \"app/deprecated/**/*\"\n  - \"packs/**/*\"\n",
    )
}

#[test]
fn test_validate_files_reports_a_bracketed_package_naming_an_unknown_team_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_a_bracketed_package_naming_an_unknown_team_in_an_allowed_glob,
            &[command, "packs/[typo]/app/thing.rb"],
        )?
        .failure()
        .stdout(predicate::str::contains("Unowned files detected:"))
        .stdout(predicate::str::contains("packs/[typo]/app/thing.rb"));
    }
    Ok(())
}

#[test]
fn test_validate_files_passes_an_allowed_file_outside_a_package_naming_an_unknown_team() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_a_bracketed_package_naming_an_unknown_team_in_an_allowed_glob,
            &[command, "packs/t/stray.rb", "app/deprecated/old.rb"],
        )?
        .success()
        .stdout(predicate::eq(""));
    }
    Ok(())
}

fn add_a_codeowner_file_naming_an_unknown_team_in_an_allowed_glob(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("app/deprecated/.codeowner"), "Typo\n")
}

#[test]
fn test_validate_files_reports_invalid_directory_teams_in_allowed_globs() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_a_codeowner_file_naming_an_unknown_team_in_an_allowed_glob,
            &[command, "app/deprecated/old.rb"],
        )?
        .failure()
        .stdout(predicate::str::contains("Unowned files detected:"))
        .stdout(predicate::str::contains("app/deprecated/old.rb"));
    }
    Ok(())
}

fn annotate_the_unowned_file_without_regenerating_codeowners(project_root: &Path) -> std::io::Result<()> {
    fs::write(
        project_root.join("app/deprecated/old.rb"),
        "# @team Foo\nputs 'no longer unowned'\n",
    )
}

#[test]
fn test_validate_files_reports_an_allowed_file_missing_from_codeowners() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(
        annotate_the_unowned_file_without_regenerating_codeowners,
        &["validate", "app/deprecated/old.rb"],
    )?
    .failure()
    .stdout(predicate::str::contains("Unowned files detected:"))
    .stdout(predicate::str::contains("app/deprecated/old.rb"));
    Ok(())
}

fn put_the_allowed_glob_in_unowned_globs_too(project_root: &Path) -> std::io::Result<()> {
    add_an_annotation_naming_an_unknown_team_in_an_allowed_glob(project_root)?;
    fs::write(
        project_root.join("config/code_ownership.yml"),
        "---\nowned_globs:\n  - \"{app,config}/**/*.rb\"\nunowned_globs:\n  - \"app/deprecated/**/*\"\nallow_unowned_globs:\n  - \"app/deprecated/**/*\"\n",
    )
}

#[test]
fn test_unowned_globs_take_precedence_over_allow_unowned_globs() -> Result<(), Box<dyn Error>> {
    // A file in both is left out, so the unknown team its annotation names doesn't count. `gv`, not `validate`, because
    // leaving files out changes CODEOWNERS.
    run_on_modified_fixture(put_the_allowed_glob_in_unowned_globs_too, &["gv"])?
        .success()
        .stdout(predicate::eq(""));
    Ok(())
}

fn put_a_codeowner_file_naming_an_unknown_team_in_both_globs(project_root: &Path) -> std::io::Result<()> {
    put_the_allowed_glob_in_unowned_globs_too(project_root)?;
    add_a_codeowner_file_naming_an_unknown_team_in_an_allowed_glob(project_root)
}

#[test]
fn test_validate_files_leaves_out_files_in_both_globs() -> Result<(), Box<dyn Error>> {
    // A `.codeowner` is read even in unowned_globs. Its team has no CODEOWNERS line, so the file passes only if it's left out.
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            put_a_codeowner_file_naming_an_unknown_team_in_both_globs,
            &[command, "app/deprecated/old.rb"],
        )?
        .success()
        .stdout(predicate::eq(""));
    }
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
