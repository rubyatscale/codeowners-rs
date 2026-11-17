//! `validate`, `for-file` and CODEOWNERS agree on which package owns a file: the nearest one, of each type.

use indoc::indoc;
use predicates::prelude::*;
use std::{error::Error, fs};
use tempfile::TempDir;

mod common;
use common::{codeowners, git_add_all_files, init_git_repo};

const RUBY_AND_TYPESCRIPT: &str = "owned_globs:\n  - \"**/*.{rb,ts}\"\n";

// A git repo of teams Foo and Bar, `config` and `files`, with CODEOWNERS generated.
fn generated_project(config: &str, files: &[(&str, &str)]) -> Result<TempDir, Box<dyn Error>> {
    let temp_dir = tempfile::tempdir()?;
    let project_root = temp_dir.path();
    let defaults = [
        ("config/code_ownership.yml", config),
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

fn assert_owner(project_root: &std::path::Path, file: &str, team: &str) -> Result<(), Box<dyn Error>> {
    for args in [["for-file", file].as_slice(), &["for-file", "--from-codeowners", file]] {
        codeowners(project_root, args)?
            .success()
            .stdout(predicate::str::starts_with(format!("Team: {team}\n")));
    }
    Ok(())
}

fn nested_packages() -> Result<TempDir, Box<dyn Error>> {
    generated_project(
        RUBY_AND_TYPESCRIPT,
        &[
            ("packs/payroll/package.yml", "owner: Foo\n"),
            ("packs/payroll/app/payroll.rb", ""),
            ("packs/payroll/payments/package.yml", "owner: Bar\n"),
            ("packs/payroll/payments/app/payment.rb", ""),
            ("frontend/app/package.json", r#"{"metadata": {"owner": "Foo"}}"#),
            ("frontend/app/index.ts", ""),
            ("frontend/app/widgets/package.json", r#"{"metadata": {"owner": "Bar"}}"#),
            ("frontend/app/widgets/button.ts", ""),
        ],
    )
}

#[test]
fn test_a_file_in_a_nested_package_belongs_to_the_nearer_package() -> Result<(), Box<dyn Error>> {
    let project = nested_packages()?;
    let project_root = project.path();
    let files = [
        ("packs/payroll/app/payroll.rb", "Foo"),
        ("packs/payroll/payments/app/payment.rb", "Bar"),
        ("frontend/app/index.ts", "Foo"),
        ("frontend/app/widgets/button.ts", "Bar"),
    ];

    for (file, team) in files {
        assert_owner(project_root, file, team)?;
    }
    codeowners(project_root, &["validate"])?.success().stdout("");
    // Only the scoped run compares each file's owner with the one CODEOWNERS gives it.
    let mut validate_files = vec!["validate"];
    validate_files.extend(files.iter().map(|(file, _)| *file));
    codeowners(project_root, &validate_files)?.success().stdout("");
    Ok(())
}

#[test]
fn test_a_file_in_a_package_inside_a_root_package_belongs_to_the_inner_package() -> Result<(), Box<dyn Error>> {
    // The default ruby_package_paths don't match the project root.
    let project = generated_project(
        "owned_globs:\n  - \"**/*.rb\"\nruby_package_paths:\n  - \"**/*\"\n",
        &[
            ("package.yml", "owner: Foo\n"),
            ("app/root.rb", ""),
            ("packs/inner/package.yml", "owner: Bar\n"),
            ("packs/inner/app/inner.rb", ""),
        ],
    )?;
    let project_root = project.path();

    assert_owner(project_root, "app/root.rb", "Foo")?;
    assert_owner(project_root, "packs/inner/app/inner.rb", "Bar")?;
    codeowners(project_root, &["validate", "app/root.rb", "packs/inner/app/inner.rb"])?
        .success()
        .stdout("");
    Ok(())
}

#[test]
fn test_validate_reports_a_nested_package_file_that_the_outer_package_team_annotates() -> Result<(), Box<dyn Error>> {
    // The annotation names the outer package's team, not the nearer package's, so the file has two owners.
    let project = generated_project(
        RUBY_AND_TYPESCRIPT,
        &[
            ("packs/payroll/package.yml", "owner: Foo\n"),
            ("packs/payroll/payments/package.yml", "owner: Bar\n"),
            ("packs/payroll/payments/app/annotated.rb", "# @team Foo\n"),
        ],
    )?;
    let project_root = project.path();

    codeowners(project_root, &["for-file", "packs/payroll/payments/app/annotated.rb"])?
        .failure()
        .stdout(predicate::str::starts_with("Error: file is owned by multiple teams!\n"));
    codeowners(project_root, &["validate"])?
        .failure()
        .stdout(predicate::str::contains(indoc! {"
        packs/payroll/payments/app/annotated.rb
         owner: Bar
          - Owner defined in `packs/payroll/payments/package.yml` with implicitly owned glob: `packs/payroll/payments/**/**`
         owner: Foo
          - Owner annotation at the top of the file
    "}));
    Ok(())
}
