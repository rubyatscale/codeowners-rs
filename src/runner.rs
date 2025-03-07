use core::fmt;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

use error_stack::{Context, Result, ResultExt};
use serde::{Deserialize, Serialize};

use crate::{
    cache::{Cache, Caching, file::GlobalCache, noop::NoopCache},
    config::Config,
    ownership::{FileOwner, Ownership},
    project::Team,
    project_builder::ProjectBuilder,
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RunResult {
    pub validation_errors: Vec<String>,
    pub io_errors: Vec<String>,
    pub info_messages: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct RunConfig {
    pub project_root: PathBuf,
    pub codeowners_file_path: PathBuf,
    pub config_path: PathBuf,
    pub no_cache: bool,
}

pub struct Runner {
    run_config: RunConfig,
    ownership: Ownership,
    cache: Cache,
}

pub fn for_file(run_config: &RunConfig, file_path: &str, fast: bool) -> RunResult {
    if fast {
        for_file_from_codeowners(run_config, file_path)
    } else {
        run_with_runner(run_config, |runner| runner.for_file(file_path))
    }
}

fn for_file_from_codeowners(run_config: &RunConfig, file_path: &str) -> RunResult {
    match team_for_file_from_codeowners(run_config, file_path) {
        Ok(Some(team)) => {
            let relative_team_yml_path = team.path.strip_prefix(&run_config.project_root).unwrap_or(&team.path);

            RunResult {
                info_messages: vec![
                    format!("Team: {}", team.name),
                    format!("Team YML: {}", relative_team_yml_path.display()),
                ],
                ..Default::default()
            }
        }
        Ok(None) => RunResult {
            info_messages: vec!["Team: Unowned".to_string(), "Team YML:".to_string()],
            ..Default::default()
        },
        Err(err) => RunResult {
            io_errors: vec![err.to_string()],
            ..Default::default()
        },
    }
}

pub fn team_for_file_from_codeowners(run_config: &RunConfig, file_path: &str) -> Result<Option<Team>, Error> {
    let config = config_from_path(&run_config.config_path)?;

    let parser = crate::ownership::parser::Parser {
        project_root: run_config.project_root.clone(),
        codeowners_file_path: run_config.codeowners_file_path.clone(),
        team_file_globs: config.team_file_glob.clone(),
    };
    Ok(parser
        .team_from_file_path(Path::new(file_path))
        .map_err(|e| Error::Io(e.to_string()))?)
}

pub fn for_team(run_config: &RunConfig, team_name: &str) -> RunResult {
    run_with_runner(run_config, |runner| runner.for_team(team_name))
}

pub fn validate(run_config: &RunConfig, _file_paths: Vec<String>) -> RunResult {
    run_with_runner(run_config, |runner| runner.validate())
}

pub fn generate(run_config: &RunConfig) -> RunResult {
    run_with_runner(run_config, |runner| runner.generate())
}

pub fn generate_and_validate(run_config: &RunConfig, _file_paths: Vec<String>) -> RunResult {
    run_with_runner(run_config, |runner| runner.generate_and_validate())
}

pub fn delete_cache(run_config: &RunConfig) -> RunResult {
    run_with_runner(run_config, |runner| runner.delete_cache())
}

pub type Runnable = fn(Runner) -> RunResult;

pub fn run_with_runner<F>(run_config: &RunConfig, runnable: F) -> RunResult
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

impl RunResult {
    pub fn has_errors(&self) -> bool {
        !self.validation_errors.is_empty() || !self.io_errors.is_empty()
    }
}

#[derive(Debug)]
pub enum Error {
    Io(String),
    ValidationFailed,
}

impl Context for Error {}
impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(msg) => fmt.write_str(msg),
            Error::ValidationFailed => fmt.write_str("Error::ValidationFailed"),
        }
    }
}

fn config_from_path(path: &PathBuf) -> Result<Config, Error> {
    let config_file = File::open(path)
        .change_context(Error::Io(format!("Can't open config file: {}", &path.to_string_lossy())))
        .attach_printable(format!("Can't open config file: {}", &path.to_string_lossy()))?;

    serde_yaml::from_reader(config_file).change_context(Error::Io(format!("Can't parse config file: {}", &path.to_string_lossy())))
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

    pub fn generate(&self) -> RunResult {
        let content = self.ownership.generate_file();
        if let Some(parent) = &self.run_config.codeowners_file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&self.run_config.codeowners_file_path, content) {
            Ok(_) => RunResult::default(),
            Err(err) => RunResult {
                io_errors: vec![err.to_string()],
                ..Default::default()
            },
        }
    }

    pub fn generate_and_validate(&self) -> RunResult {
        let run_result = self.generate();
        if run_result.has_errors() {
            return run_result;
        }
        self.validate()
    }

    pub fn for_file(&self, file_path: &str) -> RunResult {
        let file_owners = match self.ownership.for_file(file_path) {
            Ok(file_owners) => file_owners,
            Err(err) => {
                return RunResult {
                    io_errors: vec![err.to_string()],
                    ..Default::default()
                };
            }
        };
        let info_messages: Vec<String> = match file_owners.len() {
            0 => vec![format!("{}", FileOwner::default())],
            1 => vec![format!("{}", file_owners[0])],
            _ => {
                let mut error_messages = vec!["Error: file is owned by multiple teams!".to_string()];
                for file_owner in file_owners {
                    error_messages.push(format!("\n{}", file_owner));
                }
                return RunResult {
                    validation_errors: error_messages,
                    ..Default::default()
                };
            }
        };
        RunResult {
            info_messages,
            ..Default::default()
        }
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
}
