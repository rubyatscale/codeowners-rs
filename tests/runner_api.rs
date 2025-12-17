use std::path::Path;

use codeowners::runner::{self, RunConfig};

fn write_file(temp_dir: &Path, file_path: &str, content: &str) {
    let file_path = temp_dir.join(file_path);
    let _ = std::fs::create_dir_all(file_path.parent().unwrap());
    std::fs::write(file_path, content).unwrap();
}

#[test]
fn test_file_owners_for_file() {
    let temp_dir = tempfile::tempdir().unwrap();
    const DEFAULT_CODE_OWNERSHIP_YML: &str = r#"---
owned_globs:
  - "{app,components,config,frontend,lib,packs,spec,ruby}/**/*.{rb,rake,js,jsx,ts,tsx,json,yml,erb}"
unowned_globs:
  - config/code_ownership.yml
javascript_package_paths:
  - javascript/packages/**
vendored_gems_path: gems
team_file_glob:
  - config/teams/**/*.yml
"#;
    write_file(temp_dir.path(), "config/code_ownership.yml", DEFAULT_CODE_OWNERSHIP_YML);
    ["a", "b", "c"].iter().for_each(|name| {
        let team_yml = format!("name: {}\ngithub:\n  team: \"@{}\"\n  members:\n    - {}member\n", name, name, name);
        write_file(temp_dir.path(), &format!("config/teams/{}.yml", name), &team_yml);
    });
    write_file(
        temp_dir.path(),
        "app/consumers/deep/nesting/nestdir/deep_file.rb",
        "# @team b\nclass DeepFile end;",
    );

    let run_config = RunConfig {
        project_root: temp_dir.path().to_path_buf(),
        codeowners_file_path: temp_dir.path().join(".github/CODEOWNERS").to_path_buf(),
        config_path: temp_dir.path().join("config/code_ownership.yml").to_path_buf(),
        no_cache: true,
        executable_name: None,
    };

    let file_owner = runner::file_owner_for_file(&run_config, "app/consumers/deep/nesting/nestdir/deep_file.rb")
        .unwrap()
        .unwrap();
    assert_eq!(file_owner.team.name, "b");
    assert_eq!(file_owner.team.github_team, "@b");
    assert!(file_owner.team.path.to_string_lossy().ends_with("config/teams/b.yml"));
}

#[test]
fn test_teams_for_files_from_codeowners() {
    let project_root = Path::new("tests/fixtures/valid_project");
    let file_paths = [
        "javascript/packages/items/item.ts",
        "config/teams/payroll.yml",
        "ruby/app/models/bank_account.rb",
        "made/up/file.rb",
        "ruby/ignored_files/git_ignored.rb",
    ];
    let run_config = RunConfig {
        project_root: project_root.to_path_buf(),
        codeowners_file_path: project_root.join(".github/CODEOWNERS").to_path_buf(),
        config_path: project_root.join("config/code_ownership.yml").to_path_buf(),
        no_cache: true,
        executable_name: None,
    };
    let teams =
        runner::teams_for_files_from_codeowners(&run_config, &file_paths.iter().map(|s| s.to_string()).collect::<Vec<String>>()).unwrap();
    assert_eq!(teams.len(), 5);
    assert_eq!(
        teams
            .get("javascript/packages/items/item.ts")
            .unwrap()
            .as_ref()
            .map(|t| t.name.as_str()),
        Some("Payroll")
    );
    assert_eq!(
        teams.get("config/teams/payroll.yml").unwrap().as_ref().map(|t| t.name.as_str()),
        Some("Payroll")
    );
    assert_eq!(
        teams
            .get("ruby/app/models/bank_account.rb")
            .unwrap()
            .as_ref()
            .map(|t| t.name.as_str()),
        Some("Payments")
    );
    assert_eq!(teams.get("made/up/file.rb").unwrap().as_ref().map(|t| t.name.as_str()), None);
    assert_eq!(
        teams
            .get("ruby/ignored_files/git_ignored.rb")
            .unwrap()
            .as_ref()
            .map(|t| t.name.as_str()),
        None
    );
}

#[test]
fn test_for_team_reads_codeowners() {
    let td = tempfile::tempdir().unwrap();
    // minimal config
    const DEFAULT_CODE_OWNERSHIP_YML: &str = r#"---
owned_globs:
  - "app/**/*"
unowned_globs:
  - config/code_ownership.yml
team_file_glob:
  - config/teams/**/*.yml
vendored_gems_path: gems
javascript_package_paths:
  - javascript/packages/**
"#;
    write_file(td.path(), "config/code_ownership.yml", DEFAULT_CODE_OWNERSHIP_YML);

    // team file for Foo
    write_file(
        td.path(),
        "config/teams/foo.yml",
        "name: Foo\ngithub:\n  team: \"@Foo\"\n  members:\n    - user\n",
    );
    // provide a CODEOWNERS file referencing @Foo
    write_file(td.path(), ".github/CODEOWNERS", "/app/** @Foo\n");

    let rc = RunConfig {
        project_root: td.path().to_path_buf(),
        codeowners_file_path: td.path().join(".github/CODEOWNERS"),
        config_path: td.path().join("config/code_ownership.yml"),
        no_cache: true,
        executable_name: None,
    };

    // Ensure CODEOWNERS file matches generator output to avoid out-of-date errors
    let _ = runner::generate(&rc, false);
    let res = runner::for_team(&rc, "Foo");
    assert!(res.io_errors.is_empty(), "unexpected io errors: {:?}", res.io_errors);
    assert!(res.validation_errors.is_empty());
    assert!(
        res.info_messages
            .iter()
            .any(|m| m.contains("# Code Ownership Report for `Foo` Team"))
    );
}

#[test]
fn test_validate_and_generate_and_validate() {
    let td = tempfile::tempdir().unwrap();
    // config and team so generation has inputs
    const DEFAULT_CODE_OWNERSHIP_YML: &str = r#"---
owned_globs:
  - "**/*"
team_file_glob:
  - config/teams/**/*.yml
vendored_gems_path: gems
javascript_package_paths:
  - javascript/packages/**
"#;
    write_file(td.path(), "config/code_ownership.yml", DEFAULT_CODE_OWNERSHIP_YML);
    write_file(
        td.path(),
        "config/teams/foo.yml",
        "name: Foo\ngithub:\n  team: \"@Foo\"\n  members:\n    - user\nowned_globs:\n  - \"app/**\"\n  - \"config/code_ownership.yml\"\n",
    );
    // create a file to be matched (no annotation to avoid multi-source ownership)
    write_file(td.path(), "app/x.rb", "puts :x\n");

    let rc = RunConfig {
        project_root: td.path().to_path_buf(),
        codeowners_file_path: td.path().join(".github/CODEOWNERS"),
        config_path: td.path().join("config/code_ownership.yml"),
        no_cache: true,
        executable_name: None,
    };

    let gv = runner::generate_and_validate(&rc, vec![], true);
    assert!(gv.io_errors.is_empty(), "io: {:?}", gv.io_errors);
    assert!(gv.validation_errors.is_empty(), "val: {:?}", gv.validation_errors);
    // file should exist after generate
    let content = std::fs::read_to_string(td.path().join(".github/CODEOWNERS")).unwrap();
    assert!(!content.is_empty());
}
