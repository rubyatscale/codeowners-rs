use std::path::Path;

use codeowners::runner::{self, RunConfig};

mod common;
use common::{assert_no_run_errors, build_run_config, is_file_staged, setup_fixture_repo};

#[test]
fn test_generate_stages_codeowners() {
    run_and_check(runner::generate, true, true);
}

#[test]
fn test_generate_and_validate_stages_codeowners() {
    run_and_check(|rc, s| runner::generate_and_validate(rc, vec![], s), true, true);
}

#[test]
fn test_generate_does_not_stage_codeowners() {
    run_and_check(runner::generate, false, false);
}

#[test]
fn test_generate_and_validate_does_not_stage_codeowners() {
    run_and_check(|rc, s| runner::generate_and_validate(rc, vec![], s), false, false);
}

const FIXTURE: &str = "tests/fixtures/valid_project";
const CODEOWNERS_REL: &str = ".github/CODEOWNERS";

fn run_and_check<F>(func: F, stage: bool, expected_staged: bool)
where
    F: FnOnce(&RunConfig, bool) -> runner::RunResult,
{
    let temp_dir = setup_fixture_repo(Path::new(FIXTURE));
    let run_config = build_run_config(temp_dir.path(), CODEOWNERS_REL);

    let result = func(&run_config, stage);
    assert_no_run_errors(&result);

    assert!(run_config.codeowners_file_path.exists(), "CODEOWNERS file was not created");
    let staged = is_file_staged(&run_config.project_root, CODEOWNERS_REL);
    assert_eq!(staged, expected_staged, "unexpected staged state for CODEOWNERS");
}
