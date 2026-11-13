use assert_cmd::assert::Assert;
use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, ffi::OsStr, fs, path::Path};
use tempfile::TempDir;

mod common;
use common::{OutputStream, codeowners, git_add_all_files, run_codeowners, setup_fixture_repo};

const FIXTURE: &str = "optionally_owned_globs";

type FixtureChange = fn(&Path) -> std::io::Result<()>;

// A list of files is checked for invalid teams in the annotations, packages and `.codeowner` files that apply to it, as in
// `validate`, so an optionally owned file naming one is reported that way rather than excused.
fn invalid_team(file: impl std::fmt::Display) -> String {
    format!("- {file} is referencing an invalid team - 'Typo'")
}

#[test]
fn test_validate_passes_with_unowned_files_in_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    // The fixture's CODEOWNERS lists `app/deprecated/annotated_old.rb`, so if annotations in optionally owned globs stop
    // counting, this also fails, with CODEOWNERS out of date.
    run_codeowners(FIXTURE, &["validate"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_generate_and_validate_passes_with_unowned_files_in_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    run_codeowners(FIXTURE, &["gv"], true, OutputStream::Stdout, predicate::eq(""))?;
    Ok(())
}

#[test]
fn test_validate_files_passes_for_an_unowned_file_in_an_optionally_owned_glob() -> Result<(), Box<dyn Error>> {
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
fn test_validate_files_passes_for_an_absolute_path_in_an_optionally_owned_glob() -> Result<(), Box<dyn Error>> {
    validate_absolute_path(|_| Ok(()), "app/deprecated/old.rb")?
        .success()
        .stdout(predicate::eq(""));
    Ok(())
}

#[test]
fn test_validate_files_passes_for_an_annotated_and_an_unowned_file_in_an_optionally_owned_glob() -> Result<(), Box<dyn Error>> {
    // What a pre-commit hook checks when a commit edits both. An optionally owned file that names an owner is reported only if
    // CODEOWNERS doesn't list it, and CODEOWNERS lists the annotated one.
    for command in ["validate", "gv"] {
        run_codeowners(
            FIXTURE,
            &[command, "app/deprecated/annotated_old.rb", "app/deprecated/old.rb"],
            true,
            OutputStream::Stdout,
            predicate::eq(""),
        )?;
    }
    Ok(())
}

// The `for-file` tests pin that it reports an optionally owned file's owner as it would without optionally_owned_globs.
#[test]
fn test_for_file_reports_annotations_in_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
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
fn test_for_file_reports_an_unowned_file_in_an_optionally_owned_glob_as_unowned() -> Result<(), Box<dyn Error>> {
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

fn run_on_modified_fixture(modify: impl FnOnce(&Path) -> std::io::Result<()>, args: &[&str]) -> Result<Assert, Box<dyn Error>> {
    let temp_dir = modified_fixture(modify)?;
    codeowners(temp_dir.path(), args)
}

fn validate_absolute_path(modify: impl FnOnce(&Path) -> std::io::Result<()>, file: &str) -> Result<Assert, Box<dyn Error>> {
    let temp_dir = modified_fixture(modify)?;
    let file_absolute_path = temp_dir.path().join(file).canonicalize()?;
    codeowners(temp_dir.path(), &[OsStr::new("validate"), file_absolute_path.as_os_str()])
}

// A config/code_ownership.yml that replaces the fixture's. A key with no globs is left out, so unowned_globs keeps its default.
#[derive(Default)]
struct Config {
    owned_globs: &'static [&'static str],
    unowned_globs: &'static [&'static str],
    optionally_owned_globs: &'static [&'static str],
    executable_name: Option<&'static str>,
}

impl Config {
    fn write(self, project_root: &Path) -> std::io::Result<()> {
        let mut yaml = String::from("---\n");
        for (key, globs) in [
            ("owned_globs", self.owned_globs),
            ("unowned_globs", self.unowned_globs),
            ("optionally_owned_globs", self.optionally_owned_globs),
        ] {
            if !globs.is_empty() {
                yaml += &format!("{key}:\n");
                yaml.extend(globs.iter().map(|glob| format!("  - \"{glob}\"\n")));
            }
        }
        if let Some(executable_name) = self.executable_name {
            yaml += &format!("executable_name: {executable_name}\n");
        }
        fs::write(project_root.join("config/code_ownership.yml"), yaml)
    }
}

fn add_unowned_file_outside_optionally_owned_globs(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("app/stray.rb"), "puts 'no owner'\n")
}

#[test]
fn test_validate_reports_unowned_files_outside_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(add_unowned_file_outside_optionally_owned_globs, &["validate"])?
        .failure()
        .stdout(predicate::str::contains("Some files are missing ownership"))
        .stdout(predicate::str::contains("- app/stray.rb"))
        .stdout(predicate::str::contains("app/deprecated/old.rb").not());
    Ok(())
}

#[test]
fn test_validate_files_reports_an_unowned_file_outside_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(add_unowned_file_outside_optionally_owned_globs, &["validate", "app/stray.rb"])?
        .failure()
        .stdout(predicate::eq("\nSome files are missing ownership\n- app/stray.rb\n\n"));
    Ok(())
}

fn add_a_second_team_owning_the_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::write(
        project_root.join("config/teams/bar.yml"),
        "---\nname: Bar\ngithub:\n  team: '@BarTeam'\nowned_globs:\n  - app/deprecated/**/*\n",
    )
}

#[test]
fn test_validate_reports_multiple_owners_in_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(add_a_second_team_owning_the_optionally_owned_glob, &["validate"])?
        .failure()
        .stdout(predicate::str::contains(
            "Code ownership should only be defined for each file in one way.",
        ))
        .stdout(predicate::str::contains("app/deprecated/annotated_old.rb\n owner:"));
    Ok(())
}

fn add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("app/deprecated/typo.rb"), "# @team Typo\nputs 'typo'\n")
}

#[test]
fn test_validate_reports_invalid_teams_in_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob, &["validate"])?
        .failure()
        .stdout(predicate::str::contains("Found invalid team references"))
        .stdout(predicate::str::contains(
            "- app/deprecated/typo.rb is referencing an invalid team - 'Typo'",
        ));
    Ok(())
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_whose_annotation_names_an_unknown_team() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob,
            &[command, "app/deprecated/typo.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team("app/deprecated/typo.rb")
        )));
    }
    Ok(())
}

fn add_an_unowned_file_and_an_optionally_owned_one_naming_an_unknown_team(project_root: &Path) -> std::io::Result<()> {
    add_unowned_file_outside_optionally_owned_globs(project_root)?;
    add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob(project_root)
}

#[test]
fn test_validate_files_reports_optionally_owned_and_unowned_files_under_separate_headings() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_an_unowned_file_and_an_optionally_owned_one_naming_an_unknown_team,
            &[command, "app/deprecated/typo.rb", "app/stray.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\nSome files are missing ownership\n- app/stray.rb\n\n",
            invalid_team("app/deprecated/typo.rb")
        )));
    }
    Ok(())
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_whose_annotation_names_an_unknown_team_by_absolute_path()
-> Result<(), Box<dyn Error>> {
    validate_absolute_path(
        add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob,
        "app/deprecated/typo.rb",
    )?
    .failure()
    .stdout(predicate::str::contains(
        "app/deprecated/typo.rb is referencing an invalid team - 'Typo'",
    ));
    Ok(())
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_whose_annotation_names_an_unknown_team_by_path_with_dot_dot()
-> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob,
            &[command, "app/deprecated/../deprecated/typo.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team("app/deprecated/../deprecated/typo.rb")
        )));
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn test_validate_files_reports_an_optionally_owned_file_whose_annotation_names_an_unknown_team_by_path_through_a_symlink()
-> Result<(), Box<dyn Error>> {
    let temp_dir = modified_fixture(add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob)?;
    let (link, _link_dir) = common::symlink_in_temp_dir(temp_dir.path());
    let file = link.join("app/deprecated/typo.rb");
    codeowners(temp_dir.path(), &[OsStr::new("validate"), file.as_os_str()])?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team(file.display())
        )));
    Ok(())
}

fn own_every_ruby_file_and_annotate_an_optionally_owned_one_with_an_unknown_team(project_root: &Path) -> std::io::Result<()> {
    add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob(project_root)?;
    Config {
        owned_globs: &["**/*.rb"],
        optionally_owned_globs: &["app/deprecated/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

#[test]
fn test_validate_files_treats_a_dot_slash_path_like_a_plain_one() -> Result<(), Box<dyn Error>> {
    // Unlike the fixture's owned_globs, `**/*.rb` matches a path starting with `./`. The optionally owned glob doesn't, so old.rb
    // is excused only if it's matched against the plain path.
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            own_every_ruby_file_and_annotate_an_optionally_owned_one_with_an_unknown_team,
            &[command, "./app/deprecated/typo.rb"],
        )?
        .failure()
        .stdout(predicate::str::contains(invalid_team("./app/deprecated/typo.rb")));
        run_on_modified_fixture(
            own_every_ruby_file_and_annotate_an_optionally_owned_one_with_an_unknown_team,
            &[command, "./app/deprecated/old.rb"],
        )?
        .success()
        .stdout(predicate::eq(""));
    }
    Ok(())
}

fn make_every_file_optionally_owned(project_root: &Path) -> std::io::Result<()> {
    Config {
        owned_globs: &["**/*.rb"],
        optionally_owned_globs: &["**/*"],
        ..Default::default()
    }
    .write(project_root)
}

#[test]
fn test_validate_files_looks_up_an_optionally_owned_file_in_codeowners_by_its_plain_path() -> Result<(), Box<dyn Error>> {
    // Read as given, none of these paths matches annotated_old.rb's line in CODEOWNERS, and the owner lookup finds its
    // annotation.
    for command in ["validate", "gv"] {
        for file in [
            "./app/deprecated/annotated_old.rb",
            "app//deprecated/annotated_old.rb",
            "app/./deprecated/annotated_old.rb",
        ] {
            run_on_modified_fixture(make_every_file_optionally_owned, &[command, file])?
                .success()
                .stdout(predicate::eq(""));
        }
    }
    Ok(())
}

#[test]
fn test_validate_files_does_not_excuse_a_path_outside_the_project() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(make_every_file_optionally_owned, &[command, "../outside.rb"])?
            .failure()
            .stdout(predicate::eq("\nSome files are missing ownership\n- ../outside.rb\n\n"));
    }
    Ok(())
}

fn add_a_package_naming_an_unknown_team_in_an_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("packs/typo/app"))?;
    fs::write(project_root.join("packs/typo/package.yml"), "owner: Typo\n")?;
    fs::write(project_root.join("packs/typo/app/thing.rb"), "puts 'typo'\n")?;
    Config {
        owned_globs: &["{app,config,packs}/**/*.rb"],
        optionally_owned_globs: &["app/deprecated/**/*", "packs/typo/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

fn add_a_javascript_package_naming_an_unknown_team_in_an_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("frontend/typo/src"))?;
    fs::write(
        project_root.join("frontend/typo/package.json"),
        "{\"metadata\": {\"owner\": \"Typo\"}}\n",
    )?;
    fs::write(project_root.join("frontend/typo/src/thing.js"), "console.log('typo');\n")?;
    Config {
        owned_globs: &["{app,config,frontend}/**/*.{rb,js}"],
        optionally_owned_globs: &["app/deprecated/**/*", "frontend/typo/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_in_a_package_naming_an_unknown_team() -> Result<(), Box<dyn Error>> {
    let packages: [(FixtureChange, &str, &str); 2] = [
        (
            add_a_package_naming_an_unknown_team_in_an_optionally_owned_glob,
            "packs/typo/app/thing.rb",
            "packs/typo/package.yml",
        ),
        (
            add_a_javascript_package_naming_an_unknown_team_in_an_optionally_owned_glob,
            "frontend/typo/src/thing.js",
            "frontend/typo/package.json",
        ),
    ];
    for (add_package, file, package) in packages {
        for command in ["validate", "gv"] {
            run_on_modified_fixture(add_package, &[command, file])?
                .failure()
                .stdout(predicate::eq(format!(
                    "\nFound invalid team references\n{}\n\n",
                    invalid_team(package)
                )));
        }
    }
    Ok(())
}

// `packs/typo` is a string prefix of `packs/typo2` but not a path prefix, so the package names no owner for the file there.
fn add_an_optionally_owned_file_with_no_owner_in_a_sibling_of_a_package_naming_an_unknown_team(project_root: &Path) -> std::io::Result<()> {
    add_a_package_naming_an_unknown_team_in_an_optionally_owned_glob(project_root)?;
    fs::create_dir_all(project_root.join("packs/typo2/app"))?;
    fs::write(project_root.join("packs/typo2/app/thing.rb"), "puts 'no owner'\n")?;
    Config {
        owned_globs: &["{app,config,packs}/**/*.rb"],
        optionally_owned_globs: &["app/deprecated/**/*", "packs/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

#[test]
fn test_validate_files_passes_an_optionally_owned_file_in_a_sibling_of_a_package_naming_an_unknown_team() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_an_optionally_owned_file_with_no_owner_in_a_sibling_of_a_package_naming_an_unknown_team,
            &[command, "packs/typo2/app/thing.rb"],
        )?
        .success()
        .stdout(predicate::eq(""));
    }
    Ok(())
}

// Read as a glob, `packs/[typo]` would match `packs/t` rather than the package's own directory.
fn add_a_bracketed_package_naming_an_unknown_team_in_an_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("packs/[typo]/app"))?;
    fs::write(project_root.join("packs/[typo]/package.yml"), "owner: Typo\n")?;
    fs::write(project_root.join("packs/[typo]/app/thing.rb"), "puts 'typo'\n")?;
    fs::create_dir_all(project_root.join("packs/t"))?;
    fs::write(project_root.join("packs/t/stray.rb"), "puts 'no owner'\n")?;
    Config {
        owned_globs: &["{app,config,packs}/**/*.rb"],
        optionally_owned_globs: &["app/deprecated/**/*", "packs/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_in_a_bracketed_package_naming_an_unknown_team() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_a_bracketed_package_naming_an_unknown_team_in_an_optionally_owned_glob,
            &[command, "packs/[typo]/app/thing.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team("packs/[typo]/package.yml")
        )));
    }
    Ok(())
}

#[test]
fn test_validate_files_passes_an_optionally_owned_file_outside_a_package_naming_an_unknown_team() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_a_bracketed_package_naming_an_unknown_team_in_an_optionally_owned_glob,
            &[command, "packs/t/stray.rb", "app/deprecated/old.rb"],
        )?
        .success()
        .stdout(predicate::eq(""));
    }
    Ok(())
}

// Read as a glob, `packs/{typo}` would match `packs/typo` rather than the package's own directory.
fn add_a_braced_package_naming_an_unknown_team_in_an_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("packs/{typo}/app"))?;
    fs::write(project_root.join("packs/{typo}/package.yml"), "owner: Typo\n")?;
    fs::write(project_root.join("packs/{typo}/app/thing.rb"), "puts 'typo'\n")?;
    Config {
        owned_globs: &["{app,config,packs}/**/*.rb"],
        optionally_owned_globs: &["app/deprecated/**/*", "packs/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_in_a_braced_package_naming_an_unknown_team() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_a_braced_package_naming_an_unknown_team_in_an_optionally_owned_glob,
            &[command, "packs/{typo}/app/thing.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team("packs/{typo}/package.yml")
        )));
    }
    Ok(())
}

fn add_a_codeowner_file_naming_an_unknown_team_in_an_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("app/deprecated/.codeowner"), "Typo\n")
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_under_a_codeowner_naming_an_unknown_team() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_a_codeowner_file_naming_an_unknown_team_in_an_optionally_owned_glob,
            &[command, "app/deprecated/old.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team("app/deprecated/.codeowner")
        )));
    }
    Ok(())
}

// `validate` reports an empty `.codeowner` like one naming a team that doesn't exist.
fn add_an_empty_codeowner_file_in_an_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("app/deprecated/.codeowner"), "")
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_under_an_empty_codeowner() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_an_empty_codeowner_file_in_an_optionally_owned_glob,
            &[command, "app/deprecated/old.rb"],
        )?
        .failure()
        .stdout(predicate::eq(
            "\nFound invalid team references\n- app/deprecated/.codeowner is empty and names no team\n\n",
        ));
    }
    Ok(())
}

// The project root's glob is `**/**`, since `<directory>/**/**` would be `/**/**` there and match no project-relative path.
fn make_every_file_optionally_owned_and_put_a_codeowner_file_naming_an_unknown_team_at_the_project_root(
    project_root: &Path,
) -> std::io::Result<()> {
    fs::write(project_root.join(".codeowner"), "Typo\n")?;
    make_every_file_optionally_owned(project_root)
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_under_a_codeowner_at_the_root_naming_an_unknown_team() -> Result<(), Box<dyn Error>>
{
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            make_every_file_optionally_owned_and_put_a_codeowner_file_naming_an_unknown_team_at_the_project_root,
            &[command, "app/deprecated/old.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team(".codeowner")
        )));
    }
    Ok(())
}

// Unescaped, `app/deprecated/{legacy}/**/**` would match files in `app/deprecated/legacy` rather than its own directory.
fn add_a_braced_codeowner_file_naming_an_unknown_team_in_an_optionally_owned_glob(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("app/deprecated/{legacy}"))?;
    fs::write(project_root.join("app/deprecated/{legacy}/.codeowner"), "Typo\n")?;
    fs::write(project_root.join("app/deprecated/{legacy}/thing.rb"), "puts 'typo'\n")?;
    fs::create_dir_all(project_root.join("app/deprecated/legacy"))?;
    fs::write(project_root.join("app/deprecated/legacy/thing.rb"), "puts 'no owner'\n")
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_under_a_braced_codeowner_naming_an_unknown_team() -> Result<(), Box<dyn Error>> {
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            add_a_braced_codeowner_file_naming_an_unknown_team_in_an_optionally_owned_glob,
            &[command, "app/deprecated/{legacy}/thing.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team("app/deprecated/{legacy}/.codeowner")
        )));
        run_on_modified_fixture(
            add_a_braced_codeowner_file_naming_an_unknown_team_in_an_optionally_owned_glob,
            &[command, "app/deprecated/legacy/thing.rb", "app/deprecated/old.rb"],
        )?
        .success()
        .stdout(predicate::eq(""));
    }
    Ok(())
}

#[test]
fn test_validate_reports_a_package_or_codeowner_naming_an_unknown_team_in_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    // The package mappers drop the package, leaving its file with no owner, but the file is optionally owned. The `.codeowner`
    // gives annotated_old.rb a second owner, so `validate` would fail even if the `.codeowner` itself went unreported.
    let files: [(FixtureChange, &str); 2] = [
        (
            add_a_package_naming_an_unknown_team_in_an_optionally_owned_glob,
            "packs/typo/package.yml",
        ),
        (
            add_a_codeowner_file_naming_an_unknown_team_in_an_optionally_owned_glob,
            "app/deprecated/.codeowner",
        ),
    ];
    for (add_file, file) in files {
        let invalid_team = format!("- {file} is referencing an invalid team - 'Typo'");
        run_on_modified_fixture(add_file, &["validate"])?
            .failure()
            .stdout(predicate::str::contains(invalid_team))
            .stdout(predicate::str::contains("Some files are missing ownership").not());
    }
    Ok(())
}

fn annotate_the_unowned_file_without_regenerating_codeowners(project_root: &Path) -> std::io::Result<()> {
    fs::write(
        project_root.join("app/deprecated/old.rb"),
        "# @team Foo\nputs 'no longer unowned'\n",
    )
}

fn give_the_unowned_file_a_team_glob_without_regenerating_codeowners(project_root: &Path) -> std::io::Result<()> {
    fs::write(
        project_root.join("config/teams/foo.yml"),
        "---\nname: Foo\ngithub:\n  team: '@FooTeam'\nowned_globs:\n  - app/deprecated/old.rb\n",
    )
}

fn add_a_codeowner_file_without_regenerating_codeowners(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("app/deprecated/.codeowner"), "Foo\n")
}

fn add_a_package_without_regenerating_codeowners(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("packs/foo/app"))?;
    fs::write(project_root.join("packs/foo/package.yml"), "owner: Foo\n")?;
    fs::write(project_root.join("packs/foo/app/thing.rb"), "puts 'foo'\n")?;
    Config {
        owned_globs: &["{app,config,packs}/**/*.rb"],
        optionally_owned_globs: &["app/deprecated/**/*", "packs/foo/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

fn add_a_javascript_package_without_regenerating_codeowners(project_root: &Path) -> std::io::Result<()> {
    fs::create_dir_all(project_root.join("frontend/foo/src"))?;
    fs::write(
        project_root.join("frontend/foo/package.json"),
        "{\"metadata\": {\"owner\": \"Foo\"}}\n",
    )?;
    fs::write(project_root.join("frontend/foo/src/thing.js"), "console.log('foo');\n")?;
    Config {
        owned_globs: &["{app,config,frontend}/**/*.{rb,js}"],
        optionally_owned_globs: &["app/deprecated/**/*", "frontend/foo/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

#[test]
fn test_validate_files_reports_an_optionally_owned_file_missing_from_codeowners() -> Result<(), Box<dyn Error>> {
    // `validate`, not `gv`, which would add each file to CODEOWNERS.
    let owners: [(FixtureChange, &str); 5] = [
        (annotate_the_unowned_file_without_regenerating_codeowners, "app/deprecated/old.rb"),
        (
            give_the_unowned_file_a_team_glob_without_regenerating_codeowners,
            "app/deprecated/old.rb",
        ),
        (add_a_codeowner_file_without_regenerating_codeowners, "app/deprecated/old.rb"),
        (add_a_package_without_regenerating_codeowners, "packs/foo/app/thing.rb"),
        (
            add_a_javascript_package_without_regenerating_codeowners,
            "frontend/foo/src/thing.js",
        ),
    ];
    for (add_owner, file) in owners {
        run_on_modified_fixture(add_owner, &["validate", file])?
            .failure()
            .stdout(predicate::eq(format!(
                "\nCODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file\n\
                 - {file} is owned by Foo, but CODEOWNERS doesn't list it\n\n"
            )));
    }
    Ok(())
}

fn name_an_executable(project_root: &Path) -> std::io::Result<()> {
    Config {
        owned_globs: &["{app,config}/**/*.rb"],
        optionally_owned_globs: &["app/deprecated/**/*"],
        executable_name: Some("bin/codeownership validate"),
        ..Default::default()
    }
    .write(project_root)
}

fn annotate_the_unowned_file_and_name_an_executable(project_root: &Path) -> std::io::Result<()> {
    annotate_the_unowned_file_without_regenerating_codeowners(project_root)?;
    name_an_executable(project_root)
}

#[test]
fn test_validate_files_suggests_the_configured_executable_for_an_optionally_owned_file_missing_from_codeowners()
-> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(
        annotate_the_unowned_file_and_name_an_executable,
        &["validate", "app/deprecated/old.rb"],
    )?
    .failure()
    .stdout(predicate::str::contains(
        "Run `bin/codeownership validate` to update the CODEOWNERS file\n- app/deprecated/old.rb is owned by Foo",
    ));
    Ok(())
}

fn annotate_an_optionally_owned_file_with_an_unknown_team_and_name_an_executable(project_root: &Path) -> std::io::Result<()> {
    add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob(project_root)?;
    name_an_executable(project_root)
}

#[test]
fn test_generate_and_validate_files_does_not_suggest_the_configured_executable() -> Result<(), Box<dyn Error>> {
    // `gv` has just written CODEOWNERS, so running the executable to update it can't help.
    run_on_modified_fixture(
        annotate_an_optionally_owned_file_with_an_unknown_team_and_name_an_executable,
        &["gv", "app/deprecated/typo.rb"],
    )?
    .failure()
    .stdout(predicate::str::contains(invalid_team("app/deprecated/typo.rb")))
    .stdout(predicate::str::contains("to update the CODEOWNERS file").not());
    Ok(())
}

fn put_the_optionally_owned_glob_in_unowned_globs_too(project_root: &Path) -> std::io::Result<()> {
    add_an_annotation_naming_an_unknown_team_in_an_optionally_owned_glob(project_root)?;
    Config {
        owned_globs: &["{app,config}/**/*.rb"],
        unowned_globs: &["app/deprecated/**/*"],
        optionally_owned_globs: &["app/deprecated/**/*"],
        ..Default::default()
    }
    .write(project_root)
}

#[test]
fn test_unowned_globs_take_precedence_over_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    // A file in both is left out, so the unknown team its annotation names doesn't count. `gv`, not `validate`, because
    // leaving files out changes CODEOWNERS.
    run_on_modified_fixture(put_the_optionally_owned_glob_in_unowned_globs_too, &["gv"])?
        .success()
        .stdout(predicate::eq(""));
    Ok(())
}

fn put_a_codeowner_file_naming_an_unknown_team_in_both_globs(project_root: &Path) -> std::io::Result<()> {
    put_the_optionally_owned_glob_in_unowned_globs_too(project_root)?;
    add_a_codeowner_file_naming_an_unknown_team_in_an_optionally_owned_glob(project_root)
}

#[test]
fn test_validate_files_leaves_out_files_in_both_globs() -> Result<(), Box<dyn Error>> {
    // The file is left out, so it isn't reported. A `.codeowner` is read even in unowned_globs, and `validate` reports this one,
    // so a list of files in its directory does too.
    for command in ["validate", "gv"] {
        run_on_modified_fixture(
            put_a_codeowner_file_naming_an_unknown_team_in_both_globs,
            &[command, "app/deprecated/old.rb"],
        )?
        .failure()
        .stdout(predicate::eq(format!(
            "\nFound invalid team references\n{}\n\n",
            invalid_team("app/deprecated/.codeowner")
        )));
    }
    Ok(())
}

#[test]
fn test_validate_reports_unowned_files_without_optionally_owned_globs() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(
        |project_root| {
            Config {
                owned_globs: &["{app,config}/**/*.rb"],
                ..Default::default()
            }
            .write(project_root)
        },
        &["validate"],
    )?
    .failure()
    .stdout(predicate::str::contains("Some files are missing ownership"))
    .stdout(predicate::str::contains("- app/deprecated/old.rb"));
    Ok(())
}

fn make_every_file_optionally_owned_with_an_unowned_one_at_the_project_root(project_root: &Path) -> std::io::Result<()> {
    fs::write(project_root.join("root.rb"), "puts 'no owner'\n")?;
    make_every_file_optionally_owned(project_root)
}

#[test]
fn test_double_star_glob_allows_unowned_files_at_the_project_root() -> Result<(), Box<dyn Error>> {
    run_on_modified_fixture(
        make_every_file_optionally_owned_with_an_unowned_one_at_the_project_root,
        &["validate"],
    )?
    .success()
    .stdout(predicate::eq(""));
    run_on_modified_fixture(
        make_every_file_optionally_owned_with_an_unowned_one_at_the_project_root,
        &["validate", "root.rb"],
    )?
    .success()
    .stdout(predicate::eq(""));
    Ok(())
}
