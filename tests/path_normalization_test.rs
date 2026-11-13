use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, fs, path::Path};
use tempfile::TempDir;

mod common;
use common::{codeowners, git_add_all_files, init_git_repo};

// The project from #145, with CODEOWNERS generated: app/valid.rb names its team in an annotation, and app/unowned.rb has no owner.
fn project(owned_glob: &str) -> Result<TempDir, Box<dyn Error>> {
    let temp_dir = tempfile::tempdir()?;
    project_at(temp_dir.path(), &format!("owned_globs:\n  - \"{owned_glob}\"\n"))?;
    Ok(temp_dir)
}

// The same project at `root`, with `config` as its config/code_ownership.yml.
fn project_at(root: &Path, config: &str) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(root.join("app"))?;
    fs::create_dir_all(root.join("config/teams"))?;
    fs::write(root.join("config/code_ownership.yml"), config)?;
    fs::write(root.join("config/teams/valid.yml"), "name: Valid\ngithub:\n  team: '@org/valid'\n")?;
    fs::write(root.join("app/valid.rb"), "# @team Valid\n")?;
    fs::write(root.join("app/unowned.rb"), "puts 1\n")?;
    init_git_repo(root);
    git_add_all_files(root);
    codeowners(root, &["generate"])?.success();
    Ok(())
}

const FOR_FILE_FROM_ANNOTATION: &str = indoc! {"
    Team: Valid
    Github Team: @org/valid
    Team YML: config/teams/valid.yml
    Description:
    - Owner annotation at the top of the file
"};

const FOR_FILE_FROM_CODEOWNERS: &str = indoc! {"
    Team: Valid
    Github Team: @org/valid
    Team YML: config/teams/valid.yml
    Description:
    - Owner inferred from codeowners file
"};

#[test]
fn test_validate_files_finds_an_owned_file_by_any_form_of_its_path() -> Result<(), Box<dyn Error>> {
    // `**/*.rb` matches each of these as given, so each reaches the CODEOWNERS lookup, which has a line for the plain path only.
    let project = project("**/*.rb")?;
    for file in [
        "app/valid.rb",
        "./app/valid.rb",
        "app//valid.rb",
        "app/./valid.rb",
        "app/../app/valid.rb",
    ] {
        for command in ["validate", "gv"] {
            codeowners(project.path(), &[command, file])?.success().stdout(predicate::eq(""));
        }
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn test_validate_files_finds_an_owned_file_by_a_path_through_a_symlink() -> Result<(), Box<dyn Error>> {
    // The CLI canonicalizes the project root, which a path through a symlink to the project doesn't start with.
    let project = project("**/*.rb")?;
    let (link, _link_dir) = common::symlink_in_temp_dir(project.path());
    codeowners(project.path(), &["validate", link.join("app/valid.rb").to_str().unwrap()])?
        .success()
        .stdout(predicate::eq(""));
    Ok(())
}

#[test]
fn test_validate_files_reports_an_unowned_file_by_a_dot_slash_path() -> Result<(), Box<dyn Error>> {
    // `{app,config}/**/*.rb` doesn't match the path as given, so matched that way, the file would be left out and pass.
    let project = project("{app,config}/**/*.rb")?;
    for command in ["validate", "gv"] {
        codeowners(project.path(), &[command, "./app/unowned.rb"])?
            .failure()
            .stdout(predicate::eq("\nSome files are missing ownership\n- ./app/unowned.rb\n\n"));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn test_validate_files_reports_an_unowned_file_by_a_path_through_a_symlink() -> Result<(), Box<dyn Error>> {
    // A file that doesn't exist, in a directory that doesn't either, is reported as its plain path would be.
    let project = project("{app,config}/**/*.rb")?;
    let (link, _link_dir) = common::symlink_in_temp_dir(project.path());
    for file in ["app/unowned.rb", "app/deleted/unowned.rb"] {
        let path = link.join(file);
        codeowners(project.path(), &["validate", path.to_str().unwrap()])?
            .failure()
            .stdout(predicate::eq(format!(
                "\nSome files are missing ownership\n- {}\n\n",
                path.display()
            )));
    }
    Ok(())
}

#[test]
fn test_validate_files_leaves_out_a_file_in_unowned_globs_by_any_form_of_its_path() -> Result<(), Box<dyn Error>> {
    // `**/*.rb` matches each of these as given, but `app/vendor/**/*` only the plain path.
    let project = tempfile::tempdir()?;
    fs::create_dir_all(project.path().join("app/vendor"))?;
    fs::write(project.path().join("app/vendor/lib.rb"), "puts 1\n")?;
    project_at(
        project.path(),
        indoc! {r#"
            owned_globs:
              - "**/*.rb"
            unowned_globs:
              - "app/vendor/**/*"
        "#},
    )?;
    for file in ["app/vendor/lib.rb", "./app/vendor/lib.rb", "app//vendor/lib.rb"] {
        for command in ["validate", "gv"] {
            codeowners(project.path(), &[command, file])?.success().stdout(predicate::eq(""));
        }
    }
    Ok(())
}

#[test]
fn test_validate_files_reports_a_path_outside_the_project_as_unowned() -> Result<(), Box<dyn Error>> {
    // `**/*.rb` matches these as given, and CODEOWNERS has no line for them.
    let project = project("**/*.rb")?;
    let outside = project.path().parent().ok_or("temp dir has no parent")?.join("outside.rb");
    for file in ["../outside.rb", outside.to_str().unwrap()] {
        codeowners(project.path(), &["validate", file])?
            .failure()
            .stdout(predicate::eq(format!("\nSome files are missing ownership\n- {file}\n\n")));
    }
    Ok(())
}

#[test]
fn test_validate_files_reports_a_path_that_leaves_the_project_by_either_form() -> Result<(), Box<dyn Error>> {
    // Without the root, the absolute form matches `{app,config}/**/*.rb`, as the relative one does.
    let project = project("{app,config}/**/*.rb")?;
    let absolute = project.path().canonicalize()?.join("app/../../outside.rb");
    for file in ["app/../../outside.rb", absolute.to_str().unwrap()] {
        codeowners(project.path(), &["validate", file])?
            .failure()
            .stdout(predicate::eq(format!("\nSome files are missing ownership\n- {file}\n\n")));
    }
    Ok(())
}

#[test]
fn test_teams_for_files_from_codeowners_looks_up_a_path_that_leaves_the_project_alike_by_either_form() -> Result<(), Box<dyn Error>> {
    let project = project("**/*.rb")?;
    fs::create_dir_all(project.path().join("app/legacy"))?;
    fs::write(project.path().join("app/legacy/.codeowner"), "Valid\n")?;
    git_add_all_files(project.path());
    codeowners(project.path(), &["generate"])?.success();
    let run_config = common::build_run_config(project.path(), ".github/CODEOWNERS");
    let relative = "app/legacy/../../../outside.rb".to_string();
    let absolute = run_config.project_root.join(&relative).to_string_lossy().into_owned();

    let teams = codeowners::runner::teams_for_files_from_codeowners(&run_config, &[relative.clone(), absolute.clone()])?;

    let team_name = |file_path: &String| teams[file_path].as_ref().map(|team| team.name.clone());
    // CODEOWNERS has `/app/legacy/**/**`, which the relative form matches, since `**` matches `..`.
    assert_eq!(team_name(&relative).as_deref(), Some("Valid"));
    assert_eq!(team_name(&absolute), team_name(&relative));
    Ok(())
}

#[cfg(unix)]
#[test]
fn test_validate_files_and_for_file_read_dot_dot_after_a_symlinked_directory_as_its_parent() -> Result<(), Box<dyn Error>> {
    // `..` is resolved before any symlink, so this names app/valid.rb, not config/valid.rb.
    let project = project("**/*.rb")?;
    std::os::unix::fs::symlink("../config/teams", project.path().join("app/teams"))?;
    codeowners(project.path(), &["validate", "app/teams/../valid.rb"])?
        .success()
        .stdout(predicate::eq(""));
    codeowners(project.path(), &["for-file", "app/teams/../valid.rb"])?
        .success()
        .stdout(predicate::str::starts_with("Team: Valid\n"));
    Ok(())
}

#[test]
fn test_for_file_from_codeowners_finds_the_owner_by_a_dot_slash_path() -> Result<(), Box<dyn Error>> {
    let project = project("**/*.rb")?;
    codeowners(project.path(), &["for-file", "--from-codeowners", "./app/valid.rb"])?
        .success()
        .stdout(predicate::eq(FOR_FILE_FROM_CODEOWNERS));
    Ok(())
}

#[cfg(unix)]
#[test]
fn test_for_file_finds_the_owner_by_a_path_through_a_symlink() -> Result<(), Box<dyn Error>> {
    let project = project("{app,config}/**/*.rb")?;
    let (link, _link_dir) = common::symlink_in_temp_dir(project.path());
    let file = link.join("app/valid.rb");
    let file = file.to_str().unwrap();
    codeowners(project.path(), &["for-file", file])?
        .success()
        .stdout(predicate::eq(FOR_FILE_FROM_ANNOTATION));
    codeowners(project.path(), &["for-file", "--from-codeowners", file])?
        .success()
        .stdout(predicate::eq(FOR_FILE_FROM_CODEOWNERS));
    Ok(())
}

#[test]
fn test_for_file_reads_the_annotation_of_a_file_outside_the_project() -> Result<(), Box<dyn Error>> {
    // The code_ownership gem asks about files outside the project too, such as one in another gem, by an absolute or a `../` path.
    let temp_dir = tempfile::tempdir()?;
    let root = temp_dir.path().join("proj");
    project_at(&root, "owned_globs:\n  - \"**/*.rb\"\n")?;
    let outside = temp_dir.path().join("outside.rb");
    fs::write(&outside, "# @team Valid\n")?;
    for file in ["../outside.rb", outside.to_str().unwrap()] {
        codeowners(&root, &["for-file", file])?
            .success()
            .stdout(predicate::eq(FOR_FILE_FROM_ANNOTATION));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn test_teams_for_files_from_codeowners_keys_each_path_as_given() -> Result<(), Box<dyn Error>> {
    let project = project("**/*.rb")?;
    let run_config = common::build_run_config(project.path(), ".github/CODEOWNERS");
    let (link, _link_dir) = common::symlink_in_temp_dir(project.path());
    let file_paths = [
        "app/valid.rb".to_string(),
        "./app/valid.rb".to_string(),
        run_config.project_root.join("app/valid.rb").to_string_lossy().into_owned(),
        link.join("app/valid.rb").to_string_lossy().into_owned(),
    ];

    let teams = codeowners::runner::teams_for_files_from_codeowners(&run_config, &file_paths)?;

    assert_eq!(teams.len(), file_paths.len(), "{teams:?}");
    for file_path in &file_paths {
        let team = teams.get(file_path).ok_or(format!("no entry for {file_path}"))?;
        assert_eq!(team.as_ref().map(|team| team.name.as_str()), Some("Valid"), "{file_path}");
    }
    Ok(())
}
