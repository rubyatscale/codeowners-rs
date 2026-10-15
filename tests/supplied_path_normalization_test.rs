//! Normalization of the paths a caller supplies to `validate` / `generate-and-validate`.
//!
//! A supplied path has to be reduced to the project-relative form the rest of the pipeline
//! speaks. When it is not, what happens depends on the shape of `owned_globs`, and both
//! outcomes are wrong:
//!
//! - **Directory-anchored globs** (`{gems,ruby,...}/**/*.rb`, as in `valid_project`): the
//!   mishandled path matches nothing, is dropped before any ownership query runs, and the
//!   command exits 0 having checked nothing. Silent, and in the unsafe direction — a hook
//!   or CI job reports success on a file it never looked at.
//! - **`**`-leading globs** (`**/*.rb`, as in `invalid_project`): the mishandled path
//!   survives the filter and is queried in the caller's spelling, which matches no
//!   CODEOWNERS entry, so a perfectly well-owned file is reported unowned.
//!
//! Most tests here use `valid_project` and assert *failure* on a genuinely unowned file,
//! because under anchored globs a dropped path shows up as a pass. They also assert the
//! report names the **normalized** path rather than echoing the caller's spelling — without
//! that, a test cannot tell "checked correctly" from "mishandled and spuriously reported",
//! which is exactly how an earlier draft of this file passed against unfixed code.
//!
//! One test covers the `**`-glob direction, since the symptom there is the opposite. Two
//! more assert an *owned* file still passes under each odd spelling — without those, every
//! assertion here would also hold if normalization mangled one path into some other unowned
//! path.
//!
//! Path forms covered, against both states the project root can be in (resolved or not,
//! since `cli.rs` canonicalizes it but a library caller need not):
//!
//! - `./a/b.rb`, `a/c/../b.rb`   -- leading `.`, interior `..`
//! - absolute, root and path agreeing about symlinks
//! - absolute, root resolved and path not
//! - absolute, path resolved and root not (library callers only)
//! - absolute, naming a symlinked *file* -- must check the symlink, not its target
//! - a deleted path, and a path outside the project -- both skipped, deliberately

use assert_cmd::prelude::*;
use codeowners::runner::{self, RunConfig};
use predicates::prelude::*;
use std::{error::Error, process::Command};
use tempfile::TempDir;

mod common;

use common::*;

/// The project-relative form every spelling below must reduce to.
const NORMALIZED: &str = "ruby/app/unowned.rb";

/// `valid_project` with a genuinely unowned file added.
///
/// The fixture ships without one on purpose — `test_validate_with_no_files` requires it to
/// validate cleanly — so the file is injected here rather than committed. Its `owned_globs`
/// are directory-anchored, which is what makes a mishandled path get dropped, so that
/// asserting failure below is a real signal.
fn fixture_with_an_unowned_file() -> TempDir {
    let temp_dir = setup_fixture_repo(std::path::Path::new("tests/fixtures/valid_project"));
    std::fs::write(temp_dir.path().join(NORMALIZED), "# nobody owns this\n").expect("failed to write unowned file");
    git_add_all_files(temp_dir.path());
    temp_dir
}

/// Assert `validate <spelling>` succeeds, i.e. the path reached the check *and* resolved to
/// a file that really is owned.
///
/// The counterpart to `assert_normalizes`: those tests would still pass if normalization
/// mangled a path into some *other* unowned path, and these would not.
fn assert_owned_file_passes(spelling: &str) -> Result<(), Box<dyn Error>> {
    let temp_dir = fixture_with_an_unowned_file();

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(temp_dir.path())
        .arg("--no-cache")
        .arg("validate")
        .arg(spelling)
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

/// Assert `validate <spelling>` reached the ownership check and reported the file under its
/// normalized name.
fn assert_normalizes(spelling_from: impl Fn(&std::path::Path) -> String) -> Result<(), Box<dyn Error>> {
    let temp_dir = fixture_with_an_unowned_file();
    let project_root = temp_dir.path();
    let spelling = spelling_from(project_root);

    let output = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("validate")
        .arg(&spelling)
        .output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        !output.status.success(),
        "`validate {spelling}` exited 0 -- the path was dropped before any ownership query ran.\nstdout={stdout}"
    );
    assert!(
        stdout.contains(NORMALIZED),
        "report does not name the normalized path `{NORMALIZED}`.\nstdout={stdout}"
    );
    if spelling != NORMALIZED {
        assert!(
            !stdout.contains(&spelling),
            "report echoes the caller's spelling `{spelling}` instead of normalizing it, \
             which means the path reached the query unnormalized.\nstdout={stdout}"
        );
    }

    Ok(())
}

#[test]
fn test_plain_relative_path() -> Result<(), Box<dyn Error>> {
    // The control. If this fails, none of the others mean what they claim.
    assert_normalizes(|_| NORMALIZED.to_string())
}

#[test]
fn test_dot_slash_prefixed_path() -> Result<(), Box<dyn Error>> {
    assert_normalizes(|_| format!("./{NORMALIZED}"))
}

#[test]
fn test_path_with_an_interior_parent_dir() -> Result<(), Box<dyn Error>> {
    assert_normalizes(|_| "ruby/app/models/../unowned.rb".to_string())
}

#[test]
fn test_absolute_path_agreeing_with_the_root() -> Result<(), Box<dyn Error>> {
    assert_normalizes(|root| {
        root.canonicalize()
            .expect("temp dir should canonicalize")
            .join(NORMALIZED)
            .to_string_lossy()
            .to_string()
    })
}

#[test]
fn test_absolute_path_when_only_the_root_is_resolved() -> Result<(), Box<dyn Error>> {
    // `cli.rs` canonicalizes `--project-root`, so on macOS the root becomes
    // `/private/var/...` while a caller passing the `TMPDIR` spelling supplies `/var/...`.
    // `strip_prefix` then fails, the path stays absolute, and the glob filter drops it.
    // Deliberately NOT canonicalized here -- that is the case under test.
    assert_normalizes(|root| root.join(NORMALIZED).to_string_lossy().to_string())
}

#[test]
fn test_absolute_path_when_only_the_path_is_resolved() {
    // The mirror image of the test above, and it has to go through the library API: the CLI
    // always canonicalizes `--project-root`, so it cannot produce an unresolved root. A
    // library caller can, and does -- the `code_ownership` gem builds its own `RunConfig`.
    //
    // Worth its own test because resolving only the supplied path fixes the case above
    // while leaving this one failing exactly as silently.
    let temp_dir = fixture_with_an_unowned_file();
    // Deliberately NOT canonicalized -- that is the point.
    let project_root = temp_dir.path().to_path_buf();

    let canonical_file = project_root.join(NORMALIZED).canonicalize().expect("injected file should exist");

    let run_config = RunConfig {
        project_root: project_root.clone(),
        codeowners_file_path: Some(project_root.join(".github/CODEOWNERS")),
        config_path: project_root.join("config/code_ownership.yml"),
        no_cache: true,
        executable_name: None,
    };

    let result = runner::validate(&run_config, vec![canonical_file.to_string_lossy().to_string()]);

    assert!(
        result.validation_errors.iter().any(|error| error.contains(NORMALIZED)),
        "an unowned file was silently skipped: root={} file={} errors={:?}",
        project_root.display(),
        canonical_file.display(),
        result.validation_errors,
    );
}

#[test]
fn test_owned_file_passes_with_a_dot_slash_prefix() -> Result<(), Box<dyn Error>> {
    // A positive guard. Every assertion above is that an *unowned* file gets reported, which
    // would also hold if normalization mangled the path into some other unowned path. This
    // pins that a well-owned file still resolves to itself and passes.
    assert_owned_file_passes("./ruby/app/models/payroll.rb")
}

#[test]
fn test_owned_file_passes_with_an_interior_parent_dir() -> Result<(), Box<dyn Error>> {
    assert_owned_file_passes("ruby/app/payments/../models/payroll.rb")
}

// `std::os::unix::fs::symlink` has no portable equivalent, and this crate ships only
// macOS and Linux artifacts (see .github/workflows/ci.yml), so the test is gated rather
// than made portable -- `cargo test` still compiles everywhere.
#[cfg(unix)]
#[test]
fn test_absolute_path_to_a_symlink_names_the_symlink_not_its_target() {
    // Regression guard. The retry used to canonicalize the whole supplied path, which
    // follows a symlinked *file*, so an absolute path naming a symlink was checked as its
    // target -- a different file than the caller asked about. The retry now resolves only
    // the parent and re-attaches the file name, fixing the ancestor `/var` ->
    // `/private/var` discrepancy without following the leaf.
    //
    // Both the symlink and its target are unowned here, and the assertion is on *which path
    // the report names* rather than on pass/fail. An earlier version pointed the symlink at
    // an owned file and asserted failure, which was fixture-coupled and wrong: reading
    // through a symlink sees the target's contents, so once ownership is resolved through
    // the mappers rather than by reading CODEOWNERS back, the symlink genuinely inherits the
    // target's `@team` annotation and is owned. Naming the path sidesteps that entirely.
    //
    // The symlink is created after `setup_fixture_repo` because that helper copies with
    // `fs::copy`, which would follow it and write a regular file instead.
    let temp_dir = fixture_with_an_unowned_file();
    let project_root = temp_dir.path();

    let link = project_root.join("ruby/app/link_to_unowned.rb");
    std::os::unix::fs::symlink("unowned.rb", &link).expect("failed to create symlink");
    git_add_all_files(project_root);

    // Deliberately NOT canonicalized, so the retry fires.
    let absolute = link.to_string_lossy().to_string();

    let output = Command::cargo_bin("codeowners")
        .expect("binary")
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("validate")
        .arg(&absolute)
        .output()
        .expect("run");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("link_to_unowned.rb"),
        "the report names the symlink's target instead of the symlink the caller asked \
         about, so the retry followed the leaf.\nstdout={stdout}"
    );
}

#[test]
fn test_owned_file_is_not_spuriously_reported_under_star_star_globs() -> Result<(), Box<dyn Error>> {
    // The other failure mode. `invalid_project`'s `owned_globs` are `**`-leading, so a
    // mishandled path is not dropped by the filter -- it survives, gets queried in the
    // caller's spelling, matches no CODEOWNERS entry, and a well-owned file is reported
    // unowned. Same cause, opposite symptom, which is why it needs its own fixture.
    //
    // Uses `generate-and-validate` because that fixture ships an empty CODEOWNERS, so the
    // file has to be generated into it before the query can find it.
    let temp_dir = setup_fixture_repo(std::path::Path::new("tests/fixtures/invalid_project"));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate-and-validate")
        .arg("./ruby/app/models/payroll.rb")
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

#[test]
fn test_deleted_path_is_skipped() -> Result<(), Box<dyn Error>> {
    // A changeset that deletes a file lists it, so a deleted path reaches `validate` in
    // normal use. A deleted file cannot have an owner, so reporting it as unowned fails a
    // commit for removing code.
    //
    // This is the one case where dropping a supplied path silently is right -- unlike every
    // other test here, there is no file left to check. Asserted rather than assumed,
    // because "silently skipped" is otherwise exactly the bug this file is about.
    //
    // Uses `generate-and-validate` so the regenerated CODEOWNERS no longer carries the
    // deleted file's stale entry, which would otherwise mask the behavior.
    let temp_dir = setup_fixture_repo(std::path::Path::new("tests/fixtures/valid_project"));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let codeowners_path = project_root.join("tmp/CODEOWNERS");
    std::fs::remove_file(project_root.join("ruby/app/models/payroll.rb"))?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--codeowners-file-path")
        .arg(&codeowners_path)
        .arg("--no-cache")
        .arg("generate-and-validate")
        .arg("ruby/app/models/payroll.rb")
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

#[test]
fn test_path_outside_the_project_is_skipped_not_panicked_on() -> Result<(), Box<dyn Error>> {
    // A `..` that escapes the root cannot name a project file, so it is dropped rather than
    // treated as relative. Pinned mainly so it stays a skip and not a panic -- and, since the
    // filesystem retry is now gated to absolute paths, so that it stays a skip regardless of
    // the process CWD.
    let temp_dir = fixture_with_an_unowned_file();

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(temp_dir.path())
        .arg("--no-cache")
        .arg("validate")
        .arg("../outside/the_project.rb")
        .assert()
        .success();

    Ok(())
}
