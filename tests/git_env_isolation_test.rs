use std::process::Command;

mod common;

const CHILD_ENV: &str = "CODEOWNERS_GIT_ENV_TRIPWIRE_CHILD";

#[cfg(unix)]
fn cargo_runner() -> Command {
    Command::new(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("dev/without-git-repo-env"))
}

#[cfg(unix)]
#[test]
fn test_cargo_runner_strips_repo_local_git_env() {
    let temp_dir = tempfile::tempdir().unwrap();
    let vars = common::repo_local_git_env_vars();
    let mut cmd = cargo_runner();
    for var in &vars {
        cmd.env(var, temp_dir.path().join("outer.git"));
    }
    let output = cmd
        .env("GIT_EDITOR", ":")
        .arg("env")
        .output()
        .expect("failed to run the cargo runner");
    assert!(
        output.status.success(),
        "runner failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let env = String::from_utf8(output.stdout).unwrap();
    let leaked: Vec<&String> = vars
        .iter()
        .filter(|var| env.lines().any(|line| line.starts_with(&format!("{var}="))))
        .collect();
    assert!(leaked.is_empty(), "runner passed {leaked:?} through to the test binary");
    assert!(
        env.lines().any(|line| line == "GIT_EDITOR=:"),
        "runner dropped a git var that isn't repo-local"
    );
}

#[cfg(unix)]
#[test]
fn test_cargo_runner_refuses_to_run_without_git() {
    let empty_path = tempfile::tempdir().unwrap();
    let output = cargo_runner()
        .env("PATH", empty_path.path())
        .args(["/bin/echo", "ran"])
        .output()
        .expect("failed to run the cargo runner");
    assert!(!output.status.success(), "runner ran the binary without clearing git's env");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("ran"));
}

#[cfg(unix)]
#[test]
fn test_cargo_runner_preserves_the_exit_status() {
    let status = cargo_runner()
        .args(["/bin/sh", "-c", "exit 3"])
        .status()
        .expect("failed to run the cargo runner");
    assert_eq!(status.code(), Some(3), "runner must not mask a failing test binary");
}

// Re-runs the child half of `test_git_helpers_refuse_inherited_repo_env` directly, bypassing the cargo runner.
fn run_git_helper_child(envs: &[(&str, std::path::PathBuf)]) -> (bool, String) {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["test_git_helpers_refuse_inherited_repo_env", "--exact", "--test-threads=1"])
        .env(CHILD_ENV, "1");
    for (var, value) in envs {
        cmd.env(var, value);
    }
    let output = cmd.output().expect("failed to re-run the test binary");
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), log)
}

#[test]
fn test_git_helpers_refuse_inherited_repo_env() {
    if std::env::var_os(CHILD_ENV).is_some() {
        let temp_dir = tempfile::tempdir().unwrap();
        common::init_git_repo(temp_dir.path());
        return;
    }

    // A pre-commit hook in a worktree exports both, as absolute paths.
    let outer = tempfile::tempdir().unwrap();
    let outer_git_dir = outer.path().join("outer.git");
    let (succeeded, log) = run_git_helper_child(&[("GIT_DIR", outer_git_dir.clone()), ("GIT_INDEX_FILE", outer_git_dir.join("index"))]);
    assert!(!succeeded, "child should have refused: {log}");
    assert!(log.contains("refusing to run git with inherited"), "unexpected failure: {log}");
    assert!(!outer_git_dir.exists(), "test git wrote to the repo named by the inherited GIT_DIR");
}

#[test]
fn test_git_helpers_refuse_an_inherited_index_file_alone() {
    // `git commit -a` or `git commit <path>` in a regular clone exports only an absolute GIT_INDEX_FILE.
    let outer = tempfile::tempdir().unwrap();
    let outer_index = outer.path().join("index.lock");
    let (succeeded, log) = run_git_helper_child(&[("GIT_INDEX_FILE", outer_index.clone())]);
    assert!(!succeeded, "child should have refused: {log}");
    assert!(log.contains("\"GIT_INDEX_FILE\""), "tripwire didn't name GIT_INDEX_FILE: {log}");
    assert!(
        !outer_index.exists(),
        "test git wrote to the index named by the inherited GIT_INDEX_FILE"
    );
}
