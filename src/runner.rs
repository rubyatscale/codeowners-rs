use core::fmt;
use std::{
    fs::File,
    path::{Path, PathBuf},
    process::Command,
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

pub fn for_file(run_config: &RunConfig, file_path: &str, from_codeowners: bool) -> RunResult {
    if from_codeowners {
        return for_file_codeowners_only(run_config, file_path);
    }
    for_file_optimized(run_config, file_path)
}

fn for_file_codeowners_only(run_config: &RunConfig, file_path: &str) -> RunResult {
    match team_for_file_from_codeowners(run_config, file_path) {
        Ok(Some(team)) => {
            let relative_team_path = team
                .path
                .strip_prefix(&run_config.project_root)
                .unwrap_or(team.path.as_path())
                .to_string_lossy()
                .to_string();
            RunResult {
                info_messages: vec![format!(
                    "Team: {}\nGithub Team: {}\nTeam YML: {}\nDescription:\n- Owner inferred from codeowners file",
                    team.name, team.github_team, relative_team_path
                )],
                ..Default::default()
            }
        }
        Ok(None) => RunResult::default(),
        Err(err) => RunResult {
            io_errors: vec![err.to_string()],
            ..Default::default()
        },
    }
}
pub fn team_for_file_from_codeowners(run_config: &RunConfig, file_path: &str) -> Result<Option<Team>, Error> {
    let config = config_from_path(&run_config.config_path)?;
    let relative_file_path = Path::new(file_path)
        .strip_prefix(&run_config.project_root)
        .unwrap_or(Path::new(file_path));

    let parser = crate::ownership::parser::Parser {
        project_root: run_config.project_root.clone(),
        codeowners_file_path: run_config.codeowners_file_path.clone(),
        team_file_globs: config.team_file_glob.clone(),
    };
    Ok(parser
        .team_from_file_path(Path::new(relative_file_path))
        .map_err(|e| Error::Io(e.to_string()))?)
}

pub fn team_for_file(run_config: &RunConfig, file_path: &str) -> Result<Option<Team>, Error> {
    match find_owners_for_path(run_config, file_path) {
        Ok(owners) => Ok(owners.into_iter().next().map(|fo| fo.team)),
        Err(err) => Err(error_stack::Report::new(Error::Io(err))),
    }
}

pub fn file_owner_for_file(run_config: &RunConfig, file_path: &str) -> Result<Option<FileOwner>, Error> {
    match find_owners_for_path(run_config, file_path) {
        Ok(owners) => Ok(owners.into_iter().next()),
        Err(err) => Err(error_stack::Report::new(Error::Io(err))),
    }
}

// (imports below intentionally trimmed after refactor)

fn find_owners_for_path(run_config: &RunConfig, file_path: &str) -> std::result::Result<Vec<FileOwner>, String> {
    let config = match config_from_path(&run_config.config_path) {
        Ok(c) => c,
        Err(err) => return Err(err.to_string()),
    };
    crate::ownership::for_file_fast::find_file_owners(&run_config.project_root, &config, std::path::Path::new(file_path))
}

fn for_file_optimized(run_config: &RunConfig, file_path: &str) -> RunResult {
    let file_owners = match find_owners_for_path(run_config, file_path) {
        Ok(v) => v,
        Err(err) => {
            return RunResult {
                io_errors: vec![err],
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

pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn for_team(run_config: &RunConfig, team_name: &str) -> RunResult {
    run_with_runner(run_config, |runner| runner.for_team(team_name))
}

pub fn validate(run_config: &RunConfig, _file_paths: Vec<String>) -> RunResult {
    run_with_runner(run_config, |runner| runner.validate())
}

pub fn generate(run_config: &RunConfig, git_stage: bool) -> RunResult {
    run_with_runner(run_config, |runner| runner.generate(git_stage))
}

pub fn generate_and_validate(run_config: &RunConfig, _file_paths: Vec<String>, git_stage: bool) -> RunResult {
    run_with_runner(run_config, |runner| runner.generate_and_validate(git_stage))
}

pub fn delete_cache(run_config: &RunConfig) -> RunResult {
    run_with_runner(run_config, |runner| runner.delete_cache())
}

pub fn crosscheck_owners(run_config: &RunConfig) -> RunResult {
    run_with_runner(run_config, |runner| runner.crosscheck_owners())
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

pub(crate) fn config_from_path(path: &PathBuf) -> Result<Config, Error> {
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
}

#[cfg(test)]
mod tests {
    use crate::ownership::mapper::Source;

    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION").to_string());
    }
    #[test]
    fn test_file_owner_for_file() {
        let td = tempdir().unwrap();
        let project_root = td.path();

        let config_path = project_root.join("config/code_ownership.yml");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        std::fs::write(&config_path, crate::common_test::tests::DEFAULT_CODE_OWNERSHIP_YML).unwrap();
        let codeowners_file_path = project_root.join(".github/CODEOWNERS");
        std::fs::create_dir_all(codeowners_file_path.parent().unwrap()).unwrap();
        std::fs::write(&codeowners_file_path, "**/* @penny").unwrap();
        let teams_path = project_root.join("config/teams");
        std::fs::create_dir_all(&teams_path).unwrap();
        std::fs::write(
            teams_path.join("penny.yml"),
            "name: penny\ngithub:\n  team: \"@penny\"\n  members:\n    - penny\n",
        )
        .unwrap();
        let file_path = project_root.join("frontend/packages/penny/index.tsx");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "// @team penny").unwrap();
        let no_owner_file_path = project_root.join("frontend/packages/penny/no-owner-index.tsx");
        std::fs::create_dir_all(no_owner_file_path.parent().unwrap()).unwrap();
        std::fs::write(&no_owner_file_path, "").unwrap();

        let run_config = RunConfig {
            project_root: project_root.to_path_buf(),
            codeowners_file_path: project_root.join(".github/CODEOWNERS"),
            config_path,
            no_cache: true,
        };

        let file_owner = file_owner_for_file(&run_config, "frontend/packages/penny/index.tsx")
            .unwrap()
            .unwrap();
        assert_eq!(&file_owner.team.name, "penny");
        assert_eq!(&file_owner.sources, &[Source::TeamFile]);
        let team = team_for_file(&run_config, "frontend/packages/penny/index.tsx").unwrap().unwrap();
        assert_eq!(&team.name, "penny");

        let file_owner = file_owner_for_file(&run_config, "frontend/packages/penny/no-owner-index.tsx").unwrap();
        assert!(file_owner.is_none());
        let team = team_for_file(&run_config, "frontend/packages/penny/no-owner-index.tsx").unwrap();
        assert!(team.is_none());

        let file_owner = file_owner_for_file(&run_config, "madeup/file.tsx").unwrap();
        assert!(file_owner.is_none());
        let team = team_for_file(&run_config, "madeup/file.tsx").unwrap();
        assert!(team.is_none());
    }
}
