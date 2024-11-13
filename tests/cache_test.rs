use std::process::Command;

use std::error::Error;

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;

mod common;

#[test]
fn test_validate_with_cache() -> Result<(), Box<dyn Error>> {
    common::teardown();
    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("validate")
        .assert()
        .success();

    let cache_path = "tests/fixtures/valid_project/tmp/cache/codeowners/project-file-cache.json";
    assert!(std::path::Path::new(cache_path).exists(), "Cache file was not created");

    common::teardown();
    Ok(())
}
