//! Parity between `validate` / `gv` with an explicit file list and the same command with
//! no file list.
//!
//! Passing paths routes through `validate_files()` instead of `validate_all()`
//! (`runner.rs:124`). Both resolve ownership through the mappers, so both catch an
//! invalid team annotation and a file owned two ways; `validate_files` simply scopes the
//! per-file checks to the supplied paths.
//!
//! Two things differ, both deliberately. The staleness check is unavoidable: it compares
//! the whole generated CODEOWNERS against the whole on-disk one, so it cannot be scoped to
//! a subset, and `gv <paths>` makes it moot by regenerating first. The package check is
//! scoped to packages containing a supplied path, so that one bad package owner elsewhere
//! in the repo does not fail every scoped run; the `*_invalid_package*` tests pin both
//! halves of that, and the `*_untracked_*` pair pins that a path the walk never recorded is
//! asked about rather than assumed unowned.
//!
//! The `gv_*` tests previously all failed. `validate_files` used to answer only "does this
//! path have an owner in the CODEOWNERS file", which could not see a file owned two ways —
//! generation picks one winner, so the file looked owned and the command exited 0. They are
//! kept as regression guards against reintroducing that shortcut.
//!
//! Normalization of the supplied paths themselves is covered separately, in
//! `supplied_path_normalization_test.rs`.

use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::{error::Error, process::Command};

mod common;

use common::*;

/// The `invalid_project` fixture carries one defect of each class. Full `validate` reports
/// all of them; see `tests/invalid_project_test.rs`.
const FIXTURE: &str = "tests/fixtures/invalid_project";

#[test]
fn test_gv_with_paths_detects_dual_ownership_via_codeowner_file() -> Result<(), Box<dyn Error>> {
    // `ruby/app/services/multi_owned.rb` is owned twice: a `@team Payments` annotation and
    // `ruby/app/services/.codeowner` naming Payroll. Full `gv` reports "Code ownership
    // should only be defined for each file in one way".
    //
    // Regression guard. This used to exit 0 with empty output: `gv` regenerates first,
    // writing the file into CODEOWNERS under @PaymentTeam, so a check that read CODEOWNERS
    // back found an owner and passed. Regeneration concealed the defect.
    let temp_dir = setup_fixture_repo(std::path::Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .arg("ruby/app/services/multi_owned.rb")
        .assert()
        .failure()
        .stdout(predicate::str::contains("multi_owned.rb").and(predicate::str::contains("one way")));

    Ok(())
}

#[test]
fn test_gv_with_paths_detects_dual_ownership_via_owned_gems() -> Result<(), Box<dyn Error>> {
    // Same class, different source: `gems/payroll_calculator/calculator.rb` has a
    // `@team Payments` annotation while Payroll claims it through `owned_gems`.
    //
    // Regression guard, same false pass. Kept separate because the two travel through
    // different mappers, so a regression could reappear in one and not the other.
    let temp_dir = setup_fixture_repo(std::path::Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .arg("gems/payroll_calculator/calculator.rb")
        .assert()
        .failure()
        .stdout(predicate::str::contains("calculator.rb").and(predicate::str::contains("one way")));

    Ok(())
}

#[test]
fn test_gv_with_paths_names_the_invalid_team() -> Result<(), Box<dyn Error>> {
    // `ruby/app/models/blockchain.rb` is annotated `@team Web3`, which is not a team. Full
    // `gv` reports "is referencing an invalid team - 'Web3'".
    //
    // Regression guard. This used to fail, but for the wrong reason: an invalid team yields
    // no owner, so the file was absent from the generated CODEOWNERS and reported as merely
    // "unowned", sending the developer after missing ownership instead of a typo'd team.
    let temp_dir = setup_fixture_repo(std::path::Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .arg("ruby/app/models/blockchain.rb")
        .assert()
        .failure()
        .stdout(predicate::str::contains("Web3"));

    Ok(())
}

#[test]
fn test_gv_with_every_path_matches_gv_with_no_paths() -> Result<(), Box<dyn Error>> {
    // The differential check: handing over every owned file should be equivalent to handing
    // over none. This is the general form of the three tests above -- it needs no knowledge
    // of which defects the fixture contains, so it keeps working as fixtures change.
    //
    // Regression guard, and the most valuable of the set: it needs no knowledge of the
    // fixture's contents, so it keeps working as fixtures change. The all-paths run used to
    // report neither the dual ownership nor the invalid team.
    let temp_dir = setup_fixture_repo(std::path::Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    // owned_globs for this fixture is `**/*.{rb,tsx}`.
    let tracked = Command::new("git").arg("ls-files").current_dir(project_root).output()?;
    let owned_files: Vec<String> = String::from_utf8(tracked.stdout)?
        .lines()
        .filter(|line| line.ends_with(".rb") || line.ends_with(".tsx"))
        .map(str::to_string)
        .collect();
    assert!(!owned_files.is_empty(), "fixture should contain owned files");

    let no_paths = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .output()?;

    let all_paths = Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("gv")
        .args(&owned_files)
        .output()?;

    // Compare the defects each run found rather than byte-for-byte output. The two now
    // share a report format, but the no-paths run legitimately reports more (staleness,
    // and files outside the supplied list), so only the shared substance is claimed here.
    let no_paths_out = String::from_utf8_lossy(&no_paths.stdout);
    let all_paths_out = String::from_utf8_lossy(&all_paths.stdout);

    for defect in ["one way", "Web3"] {
        assert_eq!(
            no_paths_out.contains(defect),
            all_paths_out.contains(defect),
            "`gv` with no paths and `gv` with every path disagree about {:?}.\n\
             \n--- no paths (exit {:?}) ---\n{}\n--- every path (exit {:?}) ---\n{}",
            defect,
            no_paths.status.code(),
            no_paths_out,
            all_paths.status.code(),
            all_paths_out,
        );
    }

    Ok(())
}

/// Rewrite the fixture's one package manifest to name a team that does not exist, and
/// return the repo. Used by the package-scoping tests below.
fn fixture_with_an_invalid_package_owner() -> (tempfile::TempDir, std::path::PathBuf) {
    let temp_dir = setup_fixture_repo(std::path::Path::new(FIXTURE));
    let project_root = temp_dir.path().to_path_buf();

    let manifest = project_root.join("ruby/packages/payroll_flow/package.yml");
    assert!(manifest.exists(), "fixture should contain a package manifest");
    std::fs::write(&manifest, "owner: NoSuchTeam\n").expect("failed to write package manifest");

    // A file inside that package, so the in-scope case has something to name.
    let inside = project_root.join("ruby/packages/payroll_flow/app/thing.rb");
    std::fs::create_dir_all(inside.parent().unwrap()).expect("failed to create package dir");
    std::fs::write(&inside, "# inside the badly-owned package\n").expect("failed to write file");

    git_add_all_files(&project_root);
    (temp_dir, project_root)
}

#[test]
fn test_validate_with_paths_ignores_an_unrelated_invalid_package() -> Result<(), Box<dyn Error>> {
    // The package check is scoped to packages containing a supplied path. Checking every
    // package meant validating one file could fail over a package that file had nothing to
    // do with -- and since the gem's `--diff` mode feeds a changeset in, one pre-existing
    // bad package owner would block every commit in the repo until someone fixed it.
    let (_temp_dir, project_root) = fixture_with_an_invalid_package_owner();

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(&project_root)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/app/models/bank_account.rb")
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

#[test]
fn test_validate_with_paths_reports_an_invalid_package_containing_a_supplied_path() -> Result<(), Box<dyn Error>> {
    // The other half of the scoping: in scope means reported. Without this, scoping the
    // package check would just be a blind spot.
    let (_temp_dir, project_root) = fixture_with_an_invalid_package_owner();

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(&project_root)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/packages/payroll_flow/app/thing.rb")
        .assert()
        .failure()
        .stdout(predicate::str::contains("package.yml").and(predicate::str::contains("NoSuchTeam")));

    Ok(())
}

#[test]
fn test_validate_with_paths_reports_a_supplied_package_manifest() -> Result<(), Box<dyn Error>> {
    // Supplying the manifest itself has to work, or the scoped check would never catch a
    // bad package owner at the moment it is introduced -- only later, via some unrelated
    // commit that happened to touch a file inside that package. The manifest does not match
    // `owned_globs`, so it is not eligible to be reported as unowned; it is in scope purely
    // as a package selector.
    let (_temp_dir, project_root) = fixture_with_an_invalid_package_owner();

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(&project_root)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/packages/payroll_flow/package.yml")
        .assert()
        .failure()
        .stdout(
            predicate::str::contains("package.yml")
                .and(predicate::str::contains("NoSuchTeam"))
                .and(predicate::str::contains("missing ownership").not()),
        );

    Ok(())
}

#[test]
fn test_validate_with_no_paths_still_reports_every_invalid_package() -> Result<(), Box<dyn Error>> {
    // Scoping applies only to the scoped run. The whole-project run must keep reporting
    // every bad package, including the one the tests above deliberately do not name.
    let (_temp_dir, project_root) = fixture_with_an_invalid_package_owner();

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(&project_root)
        .arg("--no-cache")
        .arg("validate")
        .assert()
        .failure()
        .stdout(predicate::str::contains("package.yml").and(predicate::str::contains("NoSuchTeam")));

    Ok(())
}

#[test]
fn test_validate_attributes_an_untracked_file_through_the_mappers() -> Result<(), Box<dyn Error>> {
    // A brand-new file, not yet staged, in a directory carrying a `.codeowner`. It is
    // absent from `project.files` (the walk only records git-tracked files), but it is
    // genuinely owned -- `.codeowner` owns the directory, so the file is owned the moment
    // it exists.
    //
    // This used to report "Some files are missing ownership": paths the walk had not
    // recorded were assumed unowned rather than asked about. That put two commands in the
    // same binary at odds on the same path, since `for-file` resolves through the mappers
    // and correctly answered Payroll. It also failed in the annoying direction -- a
    // pre-commit hook rejecting a file for lacking an owner it does have.
    let temp_dir = setup_fixture_repo(std::path::Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    // `ruby/app/services/.codeowner` names Payroll. Written after `git add`, so untracked.
    let untracked = project_root.join("ruby/app/services/brand_new.rb");
    std::fs::write(&untracked, "# no annotation; owned by the directory\n")?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/app/services/brand_new.rb")
        .assert()
        .success()
        .stdout(predicate::eq(""));

    Ok(())
}

#[test]
fn test_validate_still_reports_an_untracked_file_with_no_owner() -> Result<(), Box<dyn Error>> {
    // The other side of the test above: resolving unwalked paths through the mappers must
    // not turn into "unwalked paths always pass". `ruby/app/` has no `.codeowner`, so a new
    // file there is genuinely unowned and must still be reported.
    let temp_dir = setup_fixture_repo(std::path::Path::new(FIXTURE));
    let project_root = temp_dir.path();
    git_add_all_files(project_root);

    let untracked = project_root.join("ruby/app/orphan_new.rb");
    std::fs::write(&untracked, "# nobody owns this\n")?;

    Command::cargo_bin("codeowners")?
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("validate")
        .arg("ruby/app/orphan_new.rb")
        .assert()
        .failure()
        .stdout(predicate::str::contains("orphan_new.rb").and(predicate::str::contains("missing ownership")));

    Ok(())
}
