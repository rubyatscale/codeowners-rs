//! Parity between `validate` / `gv` with a list of files and the same command with none.
//!
//! Both resolve ownership through the mappers, so both catch a file owned two ways and an invalid team. A list of files scopes
//! the checks: a package or `.codeowner` is checked only if its directory contains a listed path, so one bad one elsewhere
//! doesn't fail every scoped run, and staleness is checked file by file, since the whole-file comparison can't be scoped.
//!
//! Normalization of the given paths is covered in `path_normalization_test.rs`.

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

// Every fixture whose project can be built. missing_github_team's can't, which missing_github_team_test.rs covers.
fn buildable_fixtures() -> Vec<std::path::PathBuf> {
    std::fs::read_dir("tests/fixtures")
        .expect("tests/fixtures")
        .map(|fixture| fixture.expect("fixture").path())
        .filter(|fixture| fixture.join("config/code_ownership.yml").exists() && !fixture.ends_with("missing_github_team"))
        .collect()
}

fn tracked_files(project_root: &std::path::Path) -> Result<Vec<String>, Box<dyn Error>> {
    let tracked = Command::new("git").arg("ls-files").current_dir(project_root).output()?;
    Ok(String::from_utf8(tracked.stdout)?.lines().map(str::to_string).collect())
}

#[test]
fn test_gv_with_every_path_matches_gv_with_no_paths() -> Result<(), Box<dyn Error>> {
    // `gv` regenerates CODEOWNERS first, so nothing is stale. Given every tracked file, each package.yml and `.codeowner`
    // included, a list of files is checked for everything a full run checks.
    for fixture in buildable_fixtures() {
        let temp_dir = setup_fixture_repo(&fixture);
        let project_root = temp_dir.path();
        git_add_all_files(project_root);

        // A list of files isn't checked against `.ignore`, which leaves files out of the project walk.
        let ignored = std::fs::read_to_string(project_root.join(".ignore")).unwrap_or_default();
        let files: Vec<String> = tracked_files(project_root)?
            .into_iter()
            .filter(|file| !ignored.lines().any(|dir| std::path::Path::new(file).starts_with(dir)))
            .collect();

        let gv = |files: &[String]| {
            Command::cargo_bin("codeowners")
                .expect("codeowners binary")
                .arg("--project-root")
                .arg(project_root)
                .arg("--no-cache")
                .arg("gv")
                .arg("--skip-stage")
                .args(files)
                .output()
        };
        let no_paths = gv(&[])?;
        let every_path = gv(&files)?;

        assert_eq!(
            (every_path.status.code(), String::from_utf8_lossy(&every_path.stdout)),
            (no_paths.status.code(), String::from_utf8_lossy(&no_paths.stdout)),
            "{}",
            fixture.display()
        );
    }

    Ok(())
}

// The fixture's CODEOWNERS is empty, so a scoped `validate` also reports each file it checks as missing from it.
fn generate_codeowners(project_root: &std::path::Path) {
    Command::cargo_bin("codeowners")
        .expect("codeowners binary")
        .arg("--project-root")
        .arg(project_root)
        .arg("--no-cache")
        .arg("generate")
        .assert()
        .success();
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
    generate_codeowners(&project_root);
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
    generate_codeowners(project_root);

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

#[test]
fn test_validate_with_every_path_finds_no_stale_codeowners_line_after_generating() -> Result<(), Box<dyn Error>> {
    // Whatever else is wrong in a fixture, a CODEOWNERS file just generated is up to date, so checking each file against it
    // must not call it stale.
    for fixture in buildable_fixtures() {
        let temp_dir = setup_fixture_repo(&fixture);
        let project_root = temp_dir.path();
        git_add_all_files(project_root);
        generate_codeowners(project_root);

        let files = tracked_files(project_root)?;
        let output = Command::cargo_bin("codeowners")?
            .arg("--project-root")
            .arg(project_root)
            .arg("--no-cache")
            .arg("validate")
            .args(&files)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!stdout.contains("CODEOWNERS out of date"), "{}:\n{stdout}", fixture.display());
    }

    Ok(())
}
