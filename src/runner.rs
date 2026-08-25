use std::path::{Path, PathBuf};
use std::process::Command;

use error_stack::{Report, ResultExt};
use fast_glob::glob_match;
use serde::Serialize;
use tracing::debug_span;

use crate::{
    cache::{Cache, Caching, file::GlobalCache, noop::NoopCache},
    config::Config,
    ownership::{FileOwner, Ownership},
    project_builder::ProjectBuilder,
};

mod types;
pub use self::types::{Error, RunConfig, RunResult};
mod api;
pub use self::api::*;

pub struct Runner {
    run_config: RunConfig,
    ownership: Ownership,
    cache: Cache,
    config: Config,
    codeowners_file_path: PathBuf,
}

pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub type Runnable = fn(Runner) -> RunResult;

pub fn run<F>(run_config: &RunConfig, runnable: F) -> RunResult
where
    F: FnOnce(Runner) -> RunResult,
{
    let runner = match Runner::new(run_config) {
        Ok(runner) => runner,
        Err(err) => {
            return RunResult {
                io_errors: vec![format!("{:?}", err)],
                ..Default::default()
            };
        }
    };
    runnable(runner)
}

pub(crate) fn config_from_run_config(run_config: &RunConfig) -> Result<Config, Report<Error>> {
    match crate::config::Config::load_from_path(&run_config.config_path) {
        Ok(mut c) => {
            if let Some(executable_name) = &run_config.executable_name {
                c.executable_name = executable_name.clone();
            }
            Ok(c)
        }
        Err(msg) => Err(Report::new(Error::Io(msg))),
    }
}

/// Resolves the CODEOWNERS file path with the following priority:
/// 1. Explicit `codeowners_file_path` in `RunConfig` (if provided from e.g. CLI flag)
/// 2. `CODEOWNERS_PATH` environment variable (if set and not empty)
/// 3. Computed from `codeowners_path` directory path in config + "CODEOWNERS" filename
/// 4. Default fallback to `.github/CODEOWNERS` (using default codeowners_path from config)
pub(crate) fn resolve_codeowners_file_path(run_config: &RunConfig, config: &Config) -> PathBuf {
    if let Some(ref path) = run_config.codeowners_file_path {
        return path.clone();
    }

    if let Ok(env_path) = std::env::var("CODEOWNERS_PATH")
        && !env_path.is_empty()
    {
        return run_config.project_root.join(env_path);
    }

    run_config.project_root.join(&config.codeowners_path).join("CODEOWNERS")
}

impl Runner {
    pub fn new(run_config: &RunConfig) -> Result<Self, Report<Error>> {
        let config = debug_span!("config_load").in_scope(|| config_from_run_config(run_config))?;
        let codeowners_file_path = resolve_codeowners_file_path(run_config, &config);

        let cache: Cache = debug_span!("cache_init").in_scope(|| -> Result<Cache, Report<Error>> {
            if run_config.no_cache {
                Ok(NoopCache::default().into())
            } else {
                Ok(GlobalCache::new(run_config.project_root.clone(), config.cache_directory.clone())
                    .change_context(Error::Io(format!(
                        "Can't create cache: {}",
                        run_config.config_path.to_string_lossy()
                    )))
                    .attach(format!("Can't create cache: {}", run_config.config_path.to_string_lossy()))?
                    .into())
            }
        })?;

        let mut project_builder = ProjectBuilder::new(&config, run_config.project_root.clone(), codeowners_file_path.clone(), &cache);
        let project = project_builder.build().change_context(Error::Io(format!(
            "Can't build project: {}",
            run_config.config_path.to_string_lossy()
        )))?;
        let ownership = Ownership::build(project);

        debug_span!("cache_persist").in_scope(|| {
            cache.persist_cache().change_context(Error::Io(format!(
                "Can't persist cache: {}",
                run_config.config_path.to_string_lossy()
            )))
        })?;

        Ok(Self {
            run_config: run_config.clone(),
            ownership,
            cache,
            config,
            codeowners_file_path,
        })
    }

    pub fn validate(&self, file_paths: Vec<String>) -> RunResult {
        if file_paths.is_empty() {
            self.validate_all()
        } else {
            self.validate_files(file_paths)
        }
    }

    fn validate_all(&self) -> RunResult {
        match self.ownership.validate() {
            Ok(_) => RunResult::default(),
            Err(err) => RunResult {
                // The stale-CODEOWNERS diff (if any) rides along as informational output,
                // printed ahead of the errors, so the actionable headline isn't buried.
                info_messages: err.info_messages(),
                validation_errors: vec![format!("{}", err)],
                ..Default::default()
            },
        }
    }

    /// Validate just the supplied paths.
    ///
    /// This resolves ownership through the mappers, the same way [`Runner::validate_all`]
    /// does, rather than by reading the generated CODEOWNERS back. Reading it back could
    /// only ever answer "does this path have an owner" — it could not see a file owned two
    /// ways (generation picks one winner, so the file looks owned) nor name an annotation
    /// referencing a nonexistent team (which yields no owner, so the file merely looked
    /// unowned).
    ///
    /// Staleness is not checked here; it is a property of the whole CODEOWNERS file.
    /// `generate_and_validate` makes it moot by regenerating first.
    fn validate_files(&self, file_paths: Vec<String>) -> RunResult {
        // Normalize before anything else. `./app/x.rb`, `app/x.rb` and an absolute path to
        // the same file all have to reduce to the form `Project::relative_path` produces,
        // or the per-file checks below match nothing and the run exits 0 having checked
        // nothing -- a false pass in the unsafe direction. See
        // `path_utils::resolve_project_relative` for why an absolute path needs both sides
        // resolved, and why only its parent is.
        //
        // A path that no longer exists is dropped rather than reported: changesets delete
        // files routinely, and a deleted file cannot have an owner, so reporting it as
        // unowned would fail a commit for removing code.
        //
        // The canonical root is resolved once rather than per path, since only the retry
        // inside `resolve_project_relative` needs it and that retry can fire for every path
        // when a caller passes an absolute list.
        let canonical_root = self.run_config.project_root.canonicalize().ok();

        let supplied_paths: Vec<PathBuf> = file_paths
            .iter()
            .filter_map(|file_path| {
                crate::path_utils::resolve_project_relative(&self.run_config.project_root, canonical_root.as_deref(), Path::new(file_path))
            })
            .filter(|relative_path| {
                // `unwrap_or(true)` on purpose: only a definite "this is not there" earns a
                // silent skip. If the answer is unknown -- a permissions error, a bad
                // symlink -- keep the path and let the checks report it, because a visible
                // error is investigable and a silent pass is not.
                self.run_config.project_root.join(relative_path).try_exists().unwrap_or(true)
            })
            .collect();

        // Mirror the filtering ProjectBuilder applies when walking, so a path the project
        // would never have considered is not reported as unowned. This is narrower than
        // `supplied_paths`: a supplied package.yml or README.md is not itself an ownership
        // defect, but it does put its package in scope for the check below.
        let owned_paths: Vec<PathBuf> = supplied_paths
            .iter()
            .filter(|path| matches_globs(path, &self.config.owned_globs) && !matches_globs(path, &self.config.unowned_globs))
            .cloned()
            .collect();

        // Purely an optimization -- it skips building the mappers. With no paths in scope
        // the validator finds no files and no packages and returns Ok regardless, so this
        // is not a semantic special case. It used to be one: when the early return keyed
        // off the glob-filtered list, whether an unrelated package error surfaced depended
        // on whether some supplied path happened to match owned_globs.
        if supplied_paths.is_empty() {
            return RunResult::default();
        }

        match self.ownership.validate_files(&owned_paths, &supplied_paths) {
            Ok(_) => RunResult::default(),
            // No `info_messages`: the only error carrying one is the stale-CODEOWNERS diff,
            // which a scoped run cannot produce.
            Err(err) => RunResult {
                validation_errors: vec![format!("{}", err)],
                ..Default::default()
            },
        }
    }

    pub fn generate(&self, git_stage: bool) -> RunResult {
        let content = self.ownership.generate_file();
        if let Some(parent) = &self.codeowners_file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&self.codeowners_file_path, content) {
            Ok(_) => {
                if git_stage {
                    self.git_stage();
                }
                RunResult::default()
            }
            Err(err) => RunResult {
                io_errors: vec![err.to_string()],
                ..Default::default()
            },
        }
    }

    pub fn generate_and_validate(&self, file_paths: Vec<String>, git_stage: bool) -> RunResult {
        let run_result = self.generate(git_stage);
        if run_result.has_errors() {
            return run_result;
        }
        self.validate(file_paths)
    }

    fn git_stage(&self) {
        let _ = Command::new("git")
            .arg("add")
            .arg(&self.codeowners_file_path)
            .current_dir(&self.run_config.project_root)
            .output();
    }

    pub fn for_team(&self, team_name: &str) -> RunResult {
        let mut info_messages = vec![];
        let mut io_errors = vec![];
        match self.ownership.for_team(team_name) {
            Ok(team_ownerships) => {
                info_messages.push(format!("# Code Ownership Report for `{}` Team", team_name));
                for team_ownership in team_ownerships {
                    info_messages.push(format!("\n#{}", team_ownership.heading));
                    match team_ownership.globs.len() {
                        0 => info_messages.push("This team owns nothing in this category.".to_string()),
                        _ => info_messages.push(team_ownership.globs.join("\n")),
                    }
                }
            }
            Err(err) => io_errors.push(format!("{}", err)),
        }
        RunResult {
            info_messages,
            io_errors,
            ..Default::default()
        }
    }

    pub fn delete_cache(&self) -> RunResult {
        match self.cache.delete_cache().change_context(Error::Io(format!(
            "Can't delete cache: {}",
            self.run_config.config_path.to_string_lossy()
        ))) {
            Ok(_) => RunResult::default(),
            Err(err) => RunResult {
                io_errors: vec![err.to_string()],
                ..Default::default()
            },
        }
    }

    pub fn crosscheck_owners(&self) -> RunResult {
        crate::crosscheck::crosscheck_owners(&self.run_config, &self.cache)
    }

    pub fn owners_for_file(&self, file_path: &str) -> Result<Vec<FileOwner>, Report<Error>> {
        use crate::ownership::file_owner_resolver::find_file_owners;
        let owners = find_file_owners(&self.run_config.project_root, &self.config, std::path::Path::new(file_path)).map_err(Error::Io)?;
        Ok(owners)
    }

    pub fn for_file_derived(&self, file_path: &str, json: bool) -> RunResult {
        let file_owners = match self.owners_for_file(file_path) {
            Ok(v) => v,
            Err(err) => {
                return RunResult::from_io_error(Error::Io(err.to_string()), json);
            }
        };

        match file_owners.as_slice() {
            [] => RunResult::from_file_owner(&FileOwner::default(), json),
            [owner] => RunResult::from_file_owner(owner, json),
            many => {
                let mut error_messages = vec!["Error: file is owned by multiple teams!".to_string()];
                for owner in many {
                    error_messages.push(format!("\n{}", owner));
                }
                RunResult::from_validation_errors(error_messages, json)
            }
        }
    }

    pub fn for_file_codeowners_only(&self, file_path: &str, json: bool) -> RunResult {
        match team_for_file_from_codeowners(&self.run_config, file_path) {
            Ok(Some(team)) => {
                let team_yml = crate::path_utils::relative_to(&self.run_config.project_root, team.path.as_path())
                    .to_string_lossy()
                    .to_string();
                let result = ForFileResult {
                    team_name: team.name.clone(),
                    github_team: team.github_team.clone(),
                    team_yml,
                    description: vec!["Owner inferred from codeowners file".to_string()],
                };
                if json {
                    RunResult::json_info(result)
                } else {
                    RunResult {
                        info_messages: vec![format!(
                            "Team: {}\nGithub Team: {}\nTeam YML: {}\nDescription:\n- {}",
                            result.team_name,
                            result.github_team,
                            result.team_yml,
                            result.description.join("\n- ")
                        )],
                        ..Default::default()
                    }
                }
            }
            Ok(None) => RunResult::from_file_owner(&FileOwner::default(), json),
            Err(err) => {
                if json {
                    RunResult::json_io_error(Error::Io(err.to_string()))
                } else {
                    RunResult {
                        io_errors: vec![err.to_string()],
                        ..Default::default()
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ForFileResult {
    pub team_name: String,
    pub github_team: String,
    pub team_yml: String,
    pub description: Vec<String>,
}

impl RunResult {
    pub fn has_errors(&self) -> bool {
        !self.validation_errors.is_empty() || !self.io_errors.is_empty()
    }

    fn from_io_error(error: Error, json: bool) -> Self {
        if json {
            Self::json_io_error(error)
        } else {
            Self {
                io_errors: vec![error.to_string()],
                ..Default::default()
            }
        }
    }

    fn from_file_owner(file_owner: &FileOwner, json: bool) -> Self {
        if json {
            let description: Vec<String> = if file_owner.sources.is_empty() {
                vec![]
            } else {
                file_owner.sources.iter().map(|source| source.to_string()).collect()
            };
            Self::json_info(ForFileResult {
                team_name: file_owner.team.name.clone(),
                github_team: file_owner.team.github_team.clone(),
                team_yml: file_owner.team_config_file_path.clone(),
                description,
            })
        } else {
            Self {
                info_messages: vec![format!("{}", file_owner)],
                ..Default::default()
            }
        }
    }

    fn from_validation_errors(validation_errors: Vec<String>, json: bool) -> Self {
        if json {
            Self::json_validation_error(validation_errors)
        } else {
            Self {
                validation_errors,
                ..Default::default()
            }
        }
    }

    pub fn json_info(result: ForFileResult) -> Self {
        let json = match serde_json::to_string_pretty(&result) {
            Ok(json) => json,
            Err(e) => return Self::fallback_io_error(&e.to_string()),
        };
        Self {
            info_messages: vec![json],
            ..Default::default()
        }
    }

    pub fn json_io_error(error: Error) -> Self {
        let message = match error {
            Error::Io(msg) => msg,
            Error::ValidationFailed => "Error::ValidationFailed".to_string(),
        };
        let json = match serde_json::to_string(&serde_json::json!({"error": message})) {
            Ok(json) => json,
            Err(e) => return Self::fallback_io_error(&format!("JSON serialization failed: {}", e)),
        };
        Self {
            io_errors: vec![json],
            ..Default::default()
        }
    }

    pub fn json_validation_error(validation_errors: Vec<String>) -> Self {
        let json_obj = serde_json::json!({"validation_errors": validation_errors});
        let json = match serde_json::to_string_pretty(&json_obj) {
            Ok(json) => json,
            Err(e) => return Self::fallback_io_error(&format!("JSON serialization failed: {}", e)),
        };
        Self {
            validation_errors: vec![json],
            ..Default::default()
        }
    }

    fn fallback_io_error(message: &str) -> Self {
        Self {
            io_errors: vec![format!("{{\"error\": \"{}\"}}", message.replace('"', "\\\""))],
            ..Default::default()
        }
    }
}

/// Returns true if `path` matches any of the provided glob patterns.
fn matches_globs(path: &Path, globs: &[String]) -> bool {
    match path.to_str() {
        Some(s) => globs.iter().any(|glob| glob_match(glob, s)),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION").to_string());
    }
    #[test]
    fn test_json_info() {
        let result = ForFileResult {
            team_name: "team1".to_string(),
            github_team: "team1".to_string(),
            team_yml: "config/teams/team1.yml".to_string(),
            description: vec!["file annotation".to_string()],
        };
        let result = RunResult::json_info(result);
        assert_eq!(result.info_messages.len(), 1);
        assert_eq!(
            result.info_messages[0],
            "{\n  \"team_name\": \"team1\",\n  \"github_team\": \"team1\",\n  \"team_yml\": \"config/teams/team1.yml\",\n  \"description\": [\n    \"file annotation\"\n  ]\n}"
        );
    }

    #[test]
    fn test_json_io_error() {
        let result = RunResult::json_io_error(Error::Io("unable to find file".to_string()));
        assert_eq!(result.io_errors.len(), 1);
        assert_eq!(result.io_errors[0], "{\"error\":\"unable to find file\"}");
    }

    #[test]
    fn test_json_validation_error() {
        let result = RunResult::json_validation_error(vec!["file has multiple owners".to_string()]);
        assert_eq!(result.validation_errors.len(), 1);
        assert_eq!(
            result.validation_errors[0],
            "{\n  \"validation_errors\": [\n    \"file has multiple owners\"\n  ]\n}"
        );
    }
}
