use std::path::PathBuf;
use std::process::Command;

use std::error::Error;

use assert_cmd::assert::OutputAssertExt;
use assert_cmd::cargo::CommandCargoExt;

mod common;

#[test]
fn test_delete_cache() -> Result<(), Box<dyn Error>> {
    let cache_dir = PathBuf::from("tests/fixtures/valid_project/tmp/cache/codeowners");
    std::fs::create_dir_all(&cache_dir)?;
    let cache_path = cache_dir.join("project-file-cache.json");
    std::fs::write(&cache_path, "dummy")?;
    assert!(&cache_path.exists(), "Cache file was not created");

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg("tests/fixtures/valid_project")
        .arg("delete-cache")
        .assert()
        .success();

    assert!(!&cache_path.exists(), "Cache file was not deleted");
    common::teardown();
    Ok(())
}
