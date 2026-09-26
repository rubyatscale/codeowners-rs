// Spliced with include!() into tests/common and src/tracked_files.rs's unit tests, which can't share a module.

// The variables that pin git to one repository, per git itself.
#[allow(dead_code)]
pub fn repo_local_git_env_vars() -> Vec<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--local-env-vars"])
        .output()
        .expect("failed to run git rev-parse --local-env-vars");
    assert!(output.status.success(), "git rev-parse --local-env-vars failed");
    String::from_utf8_lossy(&output.stdout).lines().map(str::to_owned).collect()
}

// Inherited GIT_DIR/GIT_INDEX_FILE (hooks, worktrees) override current_dir and aim test git at the enclosing repo.
// Call before any test git write, including in-process `runner::generate(_, true)` and a CLI `generate`;
// the shared repo helpers in tests/common already do. Deliberately checks git's whole list, so a bypassed
// runner fails closed even on benign vars like GIT_CONFIG_PARAMETERS.
#[allow(dead_code)]
pub fn assert_git_env_isolated() {
    let leaked: Vec<String> = repo_local_git_env_vars()
        .into_iter()
        .filter(|var| std::env::var_os(var).is_some())
        .collect();
    assert!(
        leaked.is_empty(),
        "refusing to run git with inherited {leaked:?}; unset them, or run tests through this repo's cargo runner (.cargo/config.toml)"
    );
}
