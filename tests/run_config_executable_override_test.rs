use std::error::Error;
use std::path::Path;

mod common;
use common::{build_run_config, git_add_all_files, setup_fixture_repo};

#[test]
fn test_run_config_executable_path_overrides_config_file() -> Result<(), Box<dyn Error>> {
    use codeowners::runner::validate;

    let fixture_root = Path::new("tests/fixtures/custom_executable_name");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_path = temp_dir.path();
    git_add_all_files(project_path);

    // This fixture has executable_name: "bin/codeownership" in config
    // But we'll override it with RunConfig.executable_path

    let mut run_config = build_run_config(project_path, ".github/CODEOWNERS");
    // Use a relative path that gets displayed as-is in error messages
    run_config.executable_name = Some("my-wrapper-tool".to_string());

    let result = validate(&run_config, vec![]);

    // Should use "my-wrapper-tool" from executable_path, NOT "bin/codeownership" from config
    assert!(!result.validation_errors.is_empty(), "Expected validation errors but got none");
    let error_msg = result.validation_errors.join("\n");
    assert!(
        error_msg.contains("Run `my-wrapper-tool generate`"),
        "Expected error to contain 'my-wrapper-tool generate' but got: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("bin/codeownership"),
        "Error should not contain config file's executable_name when overridden"
    );

    Ok(())
}

#[test]
fn test_run_config_without_executable_path_uses_config_file() -> Result<(), Box<dyn Error>> {
    use codeowners::runner::validate;

    let fixture_root = Path::new("tests/fixtures/custom_executable_name");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_path = temp_dir.path();
    git_add_all_files(project_path);

    // This fixture has executable_name: "bin/codeownership" in config

    let mut run_config = build_run_config(project_path, ".github/CODEOWNERS");
    run_config.executable_name = None; // Explicitly no override

    let result = validate(&run_config, vec![]);

    // Should use "bin/codeownership" from config file
    assert!(!result.validation_errors.is_empty(), "Expected validation errors but got none");
    let error_msg = result.validation_errors.join("\n");
    assert!(
        error_msg.contains("Run `bin/codeownership generate`"),
        "Expected error to contain 'bin/codeownership generate' but got: {}",
        error_msg
    );

    Ok(())
}

#[test]
fn test_run_config_executable_path_overrides_default() -> Result<(), Box<dyn Error>> {
    use codeowners::runner::validate;

    let fixture_root = Path::new("tests/fixtures/default_executable_name");
    let temp_dir = setup_fixture_repo(fixture_root);
    let project_path = temp_dir.path();
    git_add_all_files(project_path);

    // This fixture has NO executable_name in config (uses default "codeowners")

    let mut run_config = build_run_config(project_path, ".github/CODEOWNERS");
    run_config.executable_name = Some("custom-command".to_string());

    let result = validate(&run_config, vec![]);

    // Should use "custom-command" from executable_path, NOT default "codeowners"
    assert!(!result.validation_errors.is_empty(), "Expected validation errors but got none");
    let error_msg = result.validation_errors.join("\n");
    assert!(
        error_msg.contains("Run `custom-command generate`"),
        "Expected error to contain 'custom-command generate' but got: {}",
        error_msg
    );
    assert!(
        !error_msg.contains("codeowners generate"),
        "Error should not contain default when overridden"
    );

    Ok(())
}
