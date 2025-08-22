use std::{path::Path, process::Command};

use error_stack::{Result, ResultExt};
use serde::Serialize;

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
                io_errors: vec![err.to_string()],
                ..Default::default()
            };
        }
    };
    runnable(runner)
}

pub(crate) fn config_from_path(path: &Path) -> Result<Config, Error> {
    match crate::config::Config::load_from_path(path) {
        Ok(c) => Ok(c),
        Err(msg) => Err(error_stack::Report::new(Error::Io(msg))),
    }
}
impl Runner {
    pub fn new(run_config: &RunConfig) -> Result<Self, Error> {
        let config = config_from_path(&run_config.config_path)?;

        let cache: Cache = if run_config.no_cache {
            NoopCache::default().into()
        } else {
            GlobalCache::new(run_config.project_root.clone(), config.cache_directory.clone())
                .change_context(Error::Io(format!(
                    "Can't create cache: {}",
                    &run_config.config_path.to_string_lossy()
                )))
                .attach_printable(format!("Can't create cache: {}", &run_config.config_path.to_string_lossy()))?
                .into()
        };

        let mut project_builder = ProjectBuilder::new(
            &config,
            run_config.project_root.clone(),
            run_config.codeowners_file_path.clone(),
            &cache,
        );
        let project = project_builder.build().change_context(Error::Io(format!(
            "Can't build project: {}",
            &run_config.config_path.to_string_lossy()
        )))?;
        let ownership = Ownership::build(project);

        cache.persist_cache().change_context(Error::Io(format!(
            "Can't persist cache: {}",
            &run_config.config_path.to_string_lossy()
        )))?;

        Ok(Self {
            run_config: run_config.clone(),
            ownership,
            cache,
            config,
        })
    }

    pub fn validate(&self) -> RunResult {
        match self.ownership.validate() {
            Ok(_) => RunResult::default(),
            Err(err) => RunResult {
                validation_errors: vec![format!("{}", err)],
                ..Default::default()
            },
        }
    }

    pub fn generate(&self, git_stage: bool) -> RunResult {
        let content = self.ownership.generate_file();
        if let Some(parent) = &self.run_config.codeowners_file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&self.run_config.codeowners_file_path, content) {
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

    pub fn generate_and_validate(&self, git_stage: bool) -> RunResult {
        let run_result = self.generate(git_stage);
        if run_result.has_errors() {
            return run_result;
        }
        self.validate()
    }

    fn git_stage(&self) {
        let _ = Command::new("git")
            .arg("add")
            .arg(&self.run_config.codeowners_file_path)
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
            &self.run_config.config_path.to_string_lossy()
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

    pub fn owners_for_file(&self, file_path: &str) -> Result<Vec<FileOwner>, Error> {
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
            Err(e) => return Self::json_io_error(Error::Io(e.to_string())),
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
        let json = match serde_json::to_string(&serde_json::json!({"error": message})).map_err(|e| Error::Io(e.to_string())) {
            Ok(json) => json,
            Err(e) => return Self::json_io_error(Error::Io(e.to_string())),
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
            Err(e) => return Self::json_io_error(Error::Io(e.to_string())),
        };
        Self {
            validation_errors: vec![json],
            ..Default::default()
        }
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
