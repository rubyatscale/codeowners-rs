use core::fmt;
use std::{fs::File, path::PathBuf};

use error_stack::{Context, Result, ResultExt};
use serde::{Deserialize, Serialize};

use crate::{
    cache::{Cache, Caching, file::GlobalCache, noop::NoopCache},
    config::Config,
    ownership::{FileOwner, Ownership, fast_team_name_from_file_path},
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

pub fn for_file(run_config: &RunConfig, file_path: &str, verbose: bool) -> RunResult {
    dbg!(verbose);
    if verbose {
        run_with_runner(run_config, |runner| runner.for_file(file_path))
    } else {
        let result = fast_team_name_from_file_path(file_path, &run_config.codeowners_file_path);
        match result {
            Ok(Some(team_name)) => RunResult {
                info_messages: vec![format!("{}", team_name)],
                ..Default::default()
            },
            Ok(None) => RunResult {
                info_messages: vec!["No team found".to_string()],
                ..Default::default()
            },
            Err(err) => RunResult {
                io_errors: vec![err.to_string()],
                ..Default::default()
            },
        }
    }
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

impl Runner {
    pub fn new(run_config: &RunConfig) -> Result<Self, Error> {
        let config_file = File::open(&run_config.config_path)
            .change_context(Error::Io(format!(
                "Can't open config file: {}",
                &run_config.config_path.to_string_lossy()
            )))
            .attach_printable(format!("Can't open config file: {}", &run_config.config_path.to_string_lossy()))?;

        let config: Config = serde_yaml::from_reader(config_file).change_context(Error::Io(format!(
            "Can't parse config file: {}",
            &run_config.config_path.to_string_lossy()
        )))?;
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
        match std::fs::write(&self.run_config.codeowners_file_path, self.ownership.generate_file()) {
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
