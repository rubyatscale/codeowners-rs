//! The globs built from a package's or `.codeowner`'s directory, or an annotated file's path, are matched by `validate` and written
//! to CODEOWNERS, where `for-file --from-codeowners` and `validate <files>` match them again.

use assert_cmd::{assert::Assert, prelude::*};
use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, fs, path::Path, process::Command};
use tempfile::TempDir;

mod common;
use common::{git_add_all_files, init_git_repo};

// The default ruby_package_paths don't match the project root.
const RUBY_PACKAGES_ANYWHERE: &str = "owned_globs:\n  - \"**/*.rb\"\nruby_package_paths:\n  - \"**/*\"\n";

// A git repo of `files`, teams Foo and Bar and a config owning `**/*.rb`, any of which `files` can replace, with CODEOWNERS
// generated.
fn generated_project(files: &[(&str, &str)]) -> Result<TempDir, Box<dyn Error>> {
    let temp_dir = tempfile::tempdir()?;
    let project_root = temp_dir.path();
    let defaults = [
        ("config/code_ownership.yml", "owned_globs:\n  - \"**/*.rb\"\n"),
        ("config/teams/foo.yml", "name: Foo\ngithub:\n  team: \"@org/foo\"\n"),
        ("config/teams/bar.yml", "name: Bar\ngithub:\n  team: \"@org/bar\"\n"),
    ];
    for (path, contents) in defaults.iter().chain(files) {
        let path = project_root.join(path);
        fs::create_dir_all(path.parent().ok_or("a file needs a parent directory")?)?;
        fs::write(path, contents)?;
    }
    init_git_repo(project_root);
    git_add_all_files(project_root);
    codeowners(project_root, &["generate"])?.success();
    Ok(temp_dir)
}

fn codeowners(project_root: &Path, args: &[&str]) -> Result<Assert, Box<dyn Error>> {
    Ok(Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .args(args)
        .assert())
}

// CODEOWNERS without its comments and blank lines.
fn codeowners_rules(project_root: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    Ok(fs::read_to_string(project_root.join(".github/CODEOWNERS"))?
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect())
}

fn assert_owner(project_root: &Path, file: &str, team: &str) -> Result<(), Box<dyn Error>> {
    for args in [["for-file", file].as_slice(), &["for-file", "--from-codeowners", file]] {
        codeowners(project_root, args)?
            .success()
            .stdout(predicate::str::starts_with(format!("Team: {team}\n")));
    }
    Ok(())
}

// The project from #146: each name with glob metacharacters has a sibling that its glob would match instead if unescaped.
fn project_with_glob_metacharacters_in_names() -> Result<TempDir, Box<dyn Error>> {
    generated_project(&[
        ("packs/[ab]/package.yml", "owner: Foo\n"),
        ("packs/{legacy}/package.yml", "owner: Foo\n"),
        ("packs/legacy/package.yml", "owner: Bar\n"),
        ("app/{old}/.codeowner", "Foo\n"),
        ("app/old/.codeowner", "Bar\n"),
        ("lib/{v2}.rb", "# @team Foo\n"),
        ("packs/[ab]/x.rb", "puts 1\n"),
        ("packs/a/y.rb", "puts 1\n"),
        ("packs/{legacy}/x.rb", "puts 1\n"),
        ("packs/legacy/z.rb", "puts 1\n"),
        ("app/{old}/x.rb", "puts 1\n"),
        ("app/old/z.rb", "puts 1\n"),
        ("lib/v2.rb", "puts 1\n"),
    ])
}

const OWNERS_OF_FILES_WITH_GLOB_METACHARACTERS_IN_NAMES: [(&str, &str); 8] = [
    ("packs/[ab]/x.rb", "Foo"),
    ("packs/a/y.rb", "Unowned"),
    ("packs/{legacy}/x.rb", "Foo"),
    ("packs/legacy/z.rb", "Bar"),
    ("app/{old}/x.rb", "Foo"),
    ("app/old/z.rb", "Bar"),
    ("lib/{v2}.rb", "Foo"),
    ("lib/v2.rb", "Unowned"),
];

#[test]
fn test_generate_escapes_glob_metacharacters_in_names() -> Result<(), Box<dyn Error>> {
    let project = project_with_glob_metacharacters_in_names()?;
    assert_eq!(
        codeowners_rules(project.path())?,
        [
            "/app/\\{old\\}/**/** @org/foo",
            "/app/old/**/** @org/bar",
            "/packs/\\[ab\\]/**/** @org/foo",
            "/packs/\\{legacy\\}/**/** @org/foo",
            "/packs/legacy/**/** @org/bar",
            "/config/teams/bar.yml @org/bar",
            "/config/teams/foo.yml @org/foo",
            "/lib/\\{v2\\}.rb @org/foo",
        ]
    );
    Ok(())
}

#[test]
fn test_for_file_agrees_with_codeowners_for_names_with_glob_metacharacters() -> Result<(), Box<dyn Error>> {
    let project = project_with_glob_metacharacters_in_names()?;
    for (file, team) in OWNERS_OF_FILES_WITH_GLOB_METACHARACTERS_IN_NAMES {
        assert_owner(project.path(), file, team)?;
    }
    codeowners(project.path(), &["crosscheck-owners"])?
        .success()
        .stdout(predicate::str::contains(
            "Success! All files match between CODEOWNERS and for-file command.",
        ));
    Ok(())
}

#[test]
fn test_for_file_describes_a_package_by_the_glob_validate_matches() -> Result<(), Box<dyn Error>> {
    let project = project_with_glob_metacharacters_in_names()?;
    codeowners(project.path(), &["for-file", "packs/{legacy}/x.rb"])?
        .success()
        .stdout(predicate::eq(indoc! {"
            Team: Foo
            Github Team: @org/foo
            Team YML: config/teams/foo.yml
            Description:
            - Owner defined in `packs/{legacy}/package.yml` with implicitly owned glob: `packs/\\{legacy\\}/**/**`
        "}));
    Ok(())
}

#[test]
fn test_validate_reads_glob_metacharacters_in_names_literally() -> Result<(), Box<dyn Error>> {
    let project = project_with_glob_metacharacters_in_names()?;
    codeowners(project.path(), &["validate"])?.failure().stdout(predicate::eq(indoc! {"

        Some files are missing ownership
        - lib/v2.rb
        - packs/a/y.rb

    "}));
    let files = OWNERS_OF_FILES_WITH_GLOB_METACHARACTERS_IN_NAMES.map(|(file, _)| file);
    codeowners(project.path(), &[["validate"].as_slice(), &files].concat())?
        .failure()
        .stdout(predicate::eq("Unowned files detected:\n  packs/a/y.rb\n  lib/v2.rb\n"));
    Ok(())
}

#[test]
fn test_validate_reads_a_leading_exclamation_mark_in_a_directory_name_literally() -> Result<(), Box<dyn Error>> {
    // Unescaped, `!legacy/**/**` would match every file not under `legacy`, so `validate` would give app/x.rb an owner.
    let project = generated_project(&[
        ("!legacy/.codeowner", "Foo\n"),
        ("!legacy/x.rb", "puts 1\n"),
        ("app/x.rb", "puts 1\n"),
    ])?;
    assert_eq!(
        codeowners_rules(project.path())?,
        [
            "/\\!legacy/**/** @org/foo",
            "/config/teams/bar.yml @org/bar",
            "/config/teams/foo.yml @org/foo",
        ]
    );
    codeowners(project.path(), &["validate"])?
        .failure()
        .stdout(predicate::eq("\nSome files are missing ownership\n- app/x.rb\n\n"));
    Ok(())
}

#[test]
fn test_a_codeowner_file_at_the_project_root_owns_every_file() -> Result<(), Box<dyn Error>> {
    let project = generated_project(&[(".codeowner", "Foo\n"), ("app/x.rb", "puts 1\n")])?;
    assert_eq!(
        codeowners_rules(project.path())?,
        [
            "/**/** @org/foo",
            "/config/teams/bar.yml @org/bar",
            "/config/teams/foo.yml @org/foo",
        ]
    );
    assert_owner(project.path(), "app/x.rb", "Foo")?;
    codeowners(project.path(), &["for-file", "app/x.rb"])?
        .success()
        .stdout(predicate::str::ends_with("Description:\n- Owner specified in `.codeowner`\n"));
    codeowners(project.path(), &["validate"])?.success().stdout(predicate::eq(""));
    codeowners(project.path(), &["validate", "app/x.rb"])?
        .success()
        .stdout(predicate::eq(""));
    Ok(())
}

#[test]
fn test_a_package_at_the_project_root_owns_every_file() -> Result<(), Box<dyn Error>> {
    let project = generated_project(&[
        ("config/code_ownership.yml", RUBY_PACKAGES_ANYWHERE),
        ("package.yml", "owner: Foo\n"),
        ("app/x.rb", "puts 1\n"),
    ])?;
    assert_eq!(
        codeowners_rules(project.path())?,
        [
            "/**/** @org/foo",
            "/config/teams/bar.yml @org/bar",
            "/config/teams/foo.yml @org/foo",
        ]
    );
    assert_owner(project.path(), "app/x.rb", "Foo")?;
    codeowners(project.path(), &["for-file", "app/x.rb"])?
        .success()
        .stdout(predicate::str::ends_with(
            "Description:\n- Owner defined in `package.yml` with implicitly owned glob: `**/**`\n",
        ));
    codeowners(project.path(), &["validate"])?.success().stdout(predicate::eq(""));
    codeowners(project.path(), &["validate", "app/x.rb"])?
        .success()
        .stdout(predicate::eq(""));
    Ok(())
}

#[test]
fn test_a_codeowner_file_in_a_directory_overrides_one_at_the_project_root() -> Result<(), Box<dyn Error>> {
    // If `validate` took the root's owner for annotated.rb, the file would have two.
    let project = generated_project(&[
        (".codeowner", "Foo\n"),
        ("app/.codeowner", "Bar\n"),
        ("app/x.rb", "puts 1\n"),
        ("app/annotated.rb", "# @team Bar\n"),
        ("lib/x.rb", "puts 1\n"),
    ])?;
    assert_eq!(
        codeowners_rules(project.path())?,
        [
            "/**/** @org/foo",
            "/app/**/** @org/bar",
            "/config/teams/bar.yml @org/bar",
            "/config/teams/foo.yml @org/foo",
            "/app/annotated.rb @org/bar",
        ]
    );
    for (file, team) in [("app/x.rb", "Bar"), ("app/annotated.rb", "Bar"), ("lib/x.rb", "Foo")] {
        assert_owner(project.path(), file, team)?;
    }
    codeowners(project.path(), &["validate"])?.success().stdout(predicate::eq(""));
    Ok(())
}

#[test]
fn test_validate_counts_a_team_reached_two_ways_as_one_owner() -> Result<(), Box<dyn Error>> {
    // If `validate` counted a team glob's or a `.codeowner`'s team by its GitHub team, each file would have two owners.
    let project = generated_project(&[
        (
            "config/teams/foo.yml",
            "name: Foo\ngithub:\n  team: \"@org/foo\"\nowned_globs:\n  - \"lib/**\"\n",
        ),
        (".codeowner", "Foo\n"),
        ("lib/x.rb", "puts 1\n"),
        ("app/.codeowner", "@org/bar\n"),
        ("app/annotated.rb", "# @team Bar\n"),
    ])?;
    for (file, team) in [("lib/x.rb", "Foo"), ("app/annotated.rb", "Bar")] {
        assert_owner(project.path(), file, team)?;
    }
    codeowners(project.path(), &["validate"])?.success().stdout(predicate::eq(""));
    Ok(())
}

#[test]
fn test_a_package_nested_in_one_at_the_project_root_owns_its_files() -> Result<(), Box<dyn Error>> {
    let project = generated_project(&[
        ("config/code_ownership.yml", RUBY_PACKAGES_ANYWHERE),
        ("package.yml", "owner: Foo\n"),
        ("packs/inner/package.yml", "owner: Bar\n"),
        ("app/x.rb", "puts 1\n"),
        ("packs/inner/y.rb", "puts 1\n"),
    ])?;
    assert_eq!(
        codeowners_rules(project.path())?,
        [
            "/**/** @org/foo",
            "/packs/inner/**/** @org/bar",
            "/config/teams/bar.yml @org/bar",
            "/config/teams/foo.yml @org/foo",
        ]
    );
    for (file, team) in [("app/x.rb", "Foo"), ("packs/inner/y.rb", "Bar")] {
        assert_owner(project.path(), file, team)?;
    }
    codeowners(project.path(), &["validate"])?.success().stdout(predicate::eq(""));
    Ok(())
}

#[test]
fn test_validate_names_the_project_root_as_what_a_codeowner_file_with_an_invalid_team_inherits_from() -> Result<(), Box<dyn Error>> {
    let project = generated_project(&[(".codeowner", "Foo\n"), ("app/.codeowner", "Typo\n"), ("app/x.rb", "puts 1\n")])?;
    codeowners(project.path(), &["validate"])?.failure().stdout(predicate::str::contains(
        "- app/.codeowner is referencing an invalid team - 'Typo'; this directory is currently inheriting its owner from the project root\n",
    ));
    Ok(())
}
