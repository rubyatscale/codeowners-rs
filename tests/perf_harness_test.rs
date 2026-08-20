//! Tests for the performance harness binary.
//!
//! These assert the harness *works* — cases execute, JSON is well-formed, the
//! corpus is restored, bad comparisons are rejected. They deliberately assert
//! nothing about how *fast* anything is: timings on a shared CI runner are
//! meaningless, and the harness is a local tool (see perf/README.md).

use std::error::Error;
use std::path::Path;

mod common;

use common::git_add_all_files;
use common::setup_fixture_repo;

fn perf_cmd() -> Result<assert_cmd::Command, Box<dyn Error>> {
    Ok(assert_cmd::Command::cargo_bin("codeowners-perf")?)
}

/// A real corpus is a committed repository. The harness refuses to run when
/// CODEOWNERS has uncommitted changes, so tests have to commit like the real
/// thing does.
fn commit_all(path: &Path) {
    git_add_all_files(path);
    let output = std::process::Command::new("git")
        .args(["commit", "-m", "fixture", "--no-verify"])
        .current_dir(path)
        .output()
        .expect("failed to run git commit");
    assert!(
        output.status.success(),
        "git commit failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Fixture corpus, committed and ready to measure.
fn corpus() -> tempfile::TempDir {
    let temp_dir = setup_fixture_repo(Path::new("tests/fixtures/valid_project"));
    commit_all(temp_dir.path());
    temp_dir
}

#[test]
fn test_lists_cases() -> Result<(), Box<dyn Error>> {
    let output = perf_cmd()?.arg("cases").assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone())?;

    for expected in ["generate", "validate_all", "gv", "validate_all_cold", "validate_files_1000"] {
        assert!(stdout.contains(expected), "case list missing {expected}: {stdout}");
    }
    Ok(())
}

#[test]
fn test_run_against_fixture_emits_well_formed_json() -> Result<(), Box<dyn Error>> {
    let temp_dir = corpus();
    let project_root = temp_dir.path();

    let output = perf_cmd()?
        .arg("run")
        .arg("--corpus")
        .arg(project_root)
        .args(["--runs", "1", "--warmup", "0", "--json"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone())?;
    let report: serde_json::Value = serde_json::from_str(&stdout)?;

    // The corpus metadata is what makes reports comparable, so it must be present.
    assert!(report["corpus"]["tracked_files"].as_u64().unwrap() > 0);
    assert!(report["corpus"]["codeowners_lines"].as_u64().unwrap() > 0);
    assert_eq!(
        report["corpus"]["smoke_scale"], true,
        "the test fixture must be reported as smoke scale"
    );

    let cases = report["cases"].as_array().expect("cases array");
    assert!(!cases.is_empty());

    // Every case either ran or explains why it did not.
    for case in cases {
        let status = case["status"].as_str().unwrap();
        assert!(status == "ok" || status == "skipped", "unexpected status {status}");
        if status == "skipped" {
            assert!(case["skip_reason"].is_string(), "skipped case must give a reason");
        } else {
            assert!(!case["runs_ms"].as_array().unwrap().is_empty());
        }
    }
    Ok(())
}

#[test]
fn test_skips_cases_the_corpus_is_too_small_for() -> Result<(), Box<dyn Error>> {
    let temp_dir = corpus();
    let project_root = temp_dir.path();

    let output = perf_cmd()?
        .arg("run")
        .arg("--corpus")
        .arg(project_root)
        .args(["--runs", "1", "--warmup", "0", "--case", "validate_files_2000", "--json"])
        .assert()
        .success();

    let stdout = String::from_utf8(output.get_output().stdout.clone())?;
    let report: serde_json::Value = serde_json::from_str(&stdout)?;
    let case = &report["cases"][0];

    // Silently measuring fewer files than the case name claims would be the worst
    // possible failure mode for a benchmark.
    assert_eq!(case["status"], "skipped");
    assert!(
        case["skip_reason"].as_str().unwrap().contains("2000"),
        "skip reason should name the shortfall: {case}"
    );
    Ok(())
}

#[test]
fn test_restores_corpus_codeowners_after_run() -> Result<(), Box<dyn Error>> {
    let temp_dir = corpus();
    let project_root = temp_dir.path();

    let codeowners = project_root.join(".github/CODEOWNERS");
    let before = std::fs::read_to_string(&codeowners)?;

    // `generate` and `gv` both write this file during the run.
    perf_cmd()?
        .arg("run")
        .arg("--corpus")
        .arg(project_root)
        .args(["--runs", "1", "--warmup", "0"])
        .assert()
        .success();

    let after = std::fs::read_to_string(&codeowners)?;
    assert_eq!(before, after, "harness must leave the corpus CODEOWNERS untouched");
    Ok(())
}

#[test]
fn test_refuses_to_run_against_dirty_corpus_codeowners() -> Result<(), Box<dyn Error>> {
    let temp_dir = corpus();
    let project_root = temp_dir.path();

    // Simulate a user with uncommitted CODEOWNERS work. If the harness were killed
    // mid-run this content would be unrecoverable, so it must refuse up front
    // rather than overwrite and hope the restore lands.
    let codeowners = project_root.join(".github/CODEOWNERS");
    std::fs::write(&codeowners, "# work in progress\n")?;

    perf_cmd()?
        .arg("run")
        .arg("--corpus")
        .arg(project_root)
        .assert()
        .failure()
        .stderr(predicates::str::contains("uncommitted changes"));

    // And the refusal must not have touched it.
    assert_eq!(std::fs::read_to_string(&codeowners)?, "# work in progress\n");
    Ok(())
}

#[test]
fn test_rejects_corpus_without_config() -> Result<(), Box<dyn Error>> {
    let temp_dir = tempfile::tempdir()?;

    perf_cmd()?
        .arg("run")
        .arg("--corpus")
        .arg(temp_dir.path())
        .assert()
        .failure()
        .stderr(predicates::str::contains("code_ownership.yml"));
    Ok(())
}

#[test]
fn test_compare_refuses_mismatched_corpora() -> Result<(), Box<dyn Error>> {
    let temp_dir = tempfile::tempdir()?;
    let baseline = temp_dir.path().join("baseline.json");
    let candidate = temp_dir.path().join("candidate.json");

    let report = |path: &str, commit: &str| {
        format!(
            r#"{{"tool_version":"0.0.0","runs_per_case":1,"cases":[],
                "corpus":{{"path":"{path}","git_commit":"{commit}","tracked_files":10,
                "owned_files":5,"codeowners_lines":5,"smoke_scale":true}}}}"#
        )
    };

    // Different corpus entirely — e.g. fixture-measured branch vs monorepo baseline.
    std::fs::write(&baseline, report("/corpus/a", "abc"))?;
    std::fs::write(&candidate, report("/corpus/b", "abc"))?;
    perf_cmd()?
        .arg("compare")
        .arg(&baseline)
        .arg(&candidate)
        .assert()
        .failure()
        .stderr(predicates::str::contains("different corpora"));

    // Same corpus, but it moved between the two measurements.
    std::fs::write(&candidate, report("/corpus/a", "def"))?;
    perf_cmd()?
        .arg("compare")
        .arg(&baseline)
        .arg(&candidate)
        .assert()
        .failure()
        .stderr(predicates::str::contains("corpus moved"));

    Ok(())
}

#[test]
fn test_compare_emits_delta_table_for_matching_corpora() -> Result<(), Box<dyn Error>> {
    let temp_dir = tempfile::tempdir()?;
    let baseline = temp_dir.path().join("baseline.json");
    let candidate = temp_dir.path().join("candidate.json");

    let report = |ms: u64| {
        format!(
            r#"{{"tool_version":"0.0.0","runs_per_case":1,
                "cases":[{{"name":"gv","status":"ok","file_count":0,"runs_ms":[{ms}],
                "best_ms":{ms},"median_ms":{ms},"phases_ms":{{}},
                "validation_errors":0,"io_errors":0}}],
                "corpus":{{"path":"/corpus/a","git_commit":"abc","tracked_files":10,
                "owned_files":5,"codeowners_lines":5,"smoke_scale":false}}}}"#
        )
    };
    std::fs::write(&baseline, report(1000))?;
    std::fs::write(&candidate, report(250))?;

    let output = perf_cmd()?.arg("compare").arg(&baseline).arg(&candidate).assert().success();

    let stdout = String::from_utf8(output.get_output().stdout.clone())?;
    assert!(stdout.contains("| gv |"), "missing case row: {stdout}");
    assert!(stdout.contains("4.00x"), "missing speedup: {stdout}");
    Ok(())
}
