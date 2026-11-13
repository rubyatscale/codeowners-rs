use assert_cmd::Command;
use predicates::prelude::*;
use std::{error::Error, fs, path::Path};
use tempfile::TempDir;

mod common;

use common::git_add_all_files;
use common::setup_fixture_repo;

const PACKAGE_YML: &str = "ruby/packages/payroll_flow/package.yml";
const TEAM_FILE: &str = "config/teams/payroll.yml";

// 30,000 ignored list items put each of `large_files` over the default `yaml_max_nodes` of 20,000.
fn project_with_large_yaml_files(large_files: &[&str], yaml_max_nodes: Option<usize>) -> TempDir {
    let temp_dir = setup_fixture_repo(Path::new("tests/fixtures/valid_project"));
    let root = temp_dir.path();
    fs::create_dir_all(root.join("ruby/packages/payroll_flow/app")).unwrap();
    fs::write(root.join("ruby/packages/payroll_flow/app/flow.rb"), "class Flow; end\n").unwrap();

    for large_file in large_files {
        let path = root.join(large_file);
        let yaml = fs::read_to_string(&path).unwrap();
        fs::write(&path, format!("{yaml}ignored: [{}]\n", vec!["x"; 30_000].join(", "))).unwrap();
    }

    if let Some(yaml_max_nodes) = yaml_max_nodes {
        let config_path = root.join("config/code_ownership.yml");
        let config = fs::read_to_string(&config_path).unwrap();
        fs::write(&config_path, format!("{config}\nyaml_max_nodes: {yaml_max_nodes}\n")).unwrap();
    }

    git_add_all_files(root);
    temp_dir
}

fn codeowners(project_root: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("codeowners").unwrap();
    cmd.arg("--project-root").arg(project_root).arg("--no-cache").args(args);
    cmd
}

#[test]
fn test_default_yaml_max_nodes_rejects_large_files() -> Result<(), Box<dyn Error>> {
    let temp_dir = project_with_large_yaml_files(&[PACKAGE_YML, TEAM_FILE], None);
    let root = temp_dir.path();

    codeowners(root, &["generate"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("budget breached").and(predicate::str::contains("raise `yaml_max_nodes` (now 20000)")));
    codeowners(root, &["for-file", "ruby/packages/payroll_flow/app/flow.rb"])
        .assert()
        .stderr(predicate::str::contains("Error reading ruby package").and(predicate::str::contains("budget breached")));
    // generate stops at whichever large file it reads first, but --from-codeowners reads only team files.
    codeowners(root, &["for-file", "--from-codeowners", "ruby/packages/payroll_flow/app/flow.rb"])
        .assert()
        .stderr(
            predicate::str::contains("Error parsing team file")
                .and(predicate::str::contains("budget breached"))
                .and(predicate::str::contains("raise `yaml_max_nodes` (now 20000)")),
        );

    Ok(())
}

#[test]
fn test_raised_yaml_max_nodes_accepts_large_files() -> Result<(), Box<dyn Error>> {
    let temp_dir = project_with_large_yaml_files(&[PACKAGE_YML, TEAM_FILE], Some(100_000));
    let root = temp_dir.path();

    codeowners(root, &["generate"]).assert().success().stderr("");
    codeowners(root, &["for-file", "ruby/packages/payroll_flow/app/flow.rb"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Team: Payroll"))
        .stderr("");
    codeowners(root, &["for-file", "--from-codeowners", "ruby/packages/payroll_flow/app/flow.rb"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Team: Payroll"))
        .stderr("");

    Ok(())
}

#[test]
fn test_hint_names_the_config_file_in_use() -> Result<(), Box<dyn Error>> {
    let hint = "raise `yaml_max_nodes` (now 20000) in config/alt.yml to allow larger files";
    // generate stops at the first large file it reads, so each project has only one.
    for large_file in [PACKAGE_YML, TEAM_FILE] {
        let temp_dir = project_with_large_yaml_files(&[large_file], None);
        let root = temp_dir.path();
        fs::rename(root.join("config/code_ownership.yml"), root.join("config/alt.yml"))?;
        let with_alt_config = |args: &[&str]| codeowners(root, &[["--config-path", "config/alt.yml"].as_slice(), args].concat());

        with_alt_config(&["generate"])
            .assert()
            .failure()
            .stderr(predicate::str::contains(hint));
        with_alt_config(&["for-file", "ruby/packages/payroll_flow/app/flow.rb"])
            .assert()
            .stderr(predicate::str::contains(hint));
        // --from-codeowners reads only team files.
        if large_file == TEAM_FILE {
            with_alt_config(&["for-file", "--from-codeowners", "ruby/packages/payroll_flow/app/flow.rb"])
                .assert()
                .stderr(predicate::str::contains(hint));
        }
    }

    Ok(())
}
