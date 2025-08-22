use std::{path::Path, process::Command};

use error_stack::{Result, ResultExt};

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
        use crate::ownership::for_file_fast::find_file_owners;
        let owners = find_file_owners(&self.run_config.project_root, &self.config, std::path::Path::new(file_path)).map_err(Error::Io)?;
        Ok(owners)
    }

    pub fn for_file_optimized(&self, file_path: &str) -> RunResult {
        let file_owners = match self.owners_for_file(file_path) {
            Ok(v) => v,
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

    pub fn for_file_codeowners_only(&self, file_path: &str) -> RunResult {
        match team_for_file_from_codeowners(&self.run_config, file_path) {
            Ok(Some(team)) => {
                let relative_team_path = team
                    .path
                    .strip_prefix(&self.run_config.project_root)
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
}

// removed free functions for for_file_* variants in favor of Runner methods

#[cfg(test)]
mod tests {
    use std::path::Path;
    use tempfile::tempdir;

    use super::*;
    use crate::{common_test, ownership::mapper::Source};

    #[test]
    fn test_version() {
        assert_eq!(version(), env!("CARGO_PKG_VERSION").to_string());
    }
    fn write_file(temp_dir: &Path, file_path: &str, content: &str) {
        let file_path = temp_dir.join(file_path);
        let _ = std::fs::create_dir_all(file_path.parent().unwrap());
        std::fs::write(file_path, content).unwrap();
    }

    #[test]
    fn test_file_owners_for_file() {
        let temp_dir = tempdir().unwrap();
        write_file(
            temp_dir.path(),
            "config/code_ownership.yml",
            common_test::tests::DEFAULT_CODE_OWNERSHIP_YML,
        );
        ["a", "b", "c"].iter().for_each(|name| {
            let team_yml = format!("name: {}\ngithub:\n  team: \"@{}\"\n  members:\n    - {}member\n", name, name, name);
            write_file(temp_dir.path(), &format!("config/teams/{}.yml", name), &team_yml);
        });
        write_file(
            temp_dir.path(),
            "app/consumers/deep/nesting/nestdir/deep_file.rb",
            "# @team b\nclass DeepFile end;",
        );

        let run_config = RunConfig {
            project_root: temp_dir.path().to_path_buf(),
            codeowners_file_path: temp_dir.path().join(".github/CODEOWNERS").to_path_buf(),
            config_path: temp_dir.path().join("config/code_ownership.yml").to_path_buf(),
            no_cache: false,
        };

        let file_owner = file_owner_for_file(&run_config, "app/consumers/deep/nesting/nestdir/deep_file.rb")
            .unwrap()
            .unwrap();
        assert_eq!(file_owner.team.name, "b");
        assert_eq!(file_owner.team.github_team, "@b");
        assert!(file_owner.team.path.to_string_lossy().ends_with("config/teams/b.yml"));
        assert_eq!(file_owner.sources.len(), 1);
        assert_eq!(file_owner.sources, vec![Source::AnnotatedFile]);

        let team = team_for_file(&run_config, "app/consumers/deep/nesting/nestdir/deep_file.rb")
            .unwrap()
            .unwrap();
        assert_eq!(team.name, "b");
        assert_eq!(team.github_team, "@b");
        assert!(team.path.to_string_lossy().ends_with("config/teams/b.yml"));
    }

    #[test]
    fn test_teams_for_files_from_codeowners() {
        let project_root = Path::new("tests/fixtures/valid_project");
        let file_paths = [
            "javascript/packages/items/item.ts",
            "config/teams/payroll.yml",
            "ruby/app/models/bank_account.rb",
            "made/up/file.rb",
            "ruby/ignored_files/git_ignored.rb",
        ];
        let run_config = RunConfig {
            project_root: project_root.to_path_buf(),
            codeowners_file_path: project_root.join(".github/CODEOWNERS").to_path_buf(),
            config_path: project_root.join("config/code_ownership.yml").to_path_buf(),
            no_cache: false,
        };
        let teams =
            teams_for_files_from_codeowners(&run_config, &file_paths.iter().map(|s| s.to_string()).collect::<Vec<String>>()).unwrap();
        assert_eq!(teams.len(), 5);
        assert_eq!(
            teams
                .get("javascript/packages/items/item.ts")
                .unwrap()
                .as_ref()
                .map(|t| t.name.as_str()),
            Some("Payroll")
        );
        assert_eq!(
            teams.get("config/teams/payroll.yml").unwrap().as_ref().map(|t| t.name.as_str()),
            Some("Payroll")
        );
        assert_eq!(
            teams
                .get("ruby/app/models/bank_account.rb")
                .unwrap()
                .as_ref()
                .map(|t| t.name.as_str()),
            Some("Payments")
        );
        assert_eq!(teams.get("made/up/file.rb").unwrap().as_ref().map(|t| t.name.as_str()), None);
        assert_eq!(
            teams
                .get("ruby/ignored_files/git_ignored.rb")
                .unwrap()
                .as_ref()
                .map(|t| t.name.as_str()),
            None
        );
        assert_eq!(
            teams
                .get("ruby/ignored_files/git_ignored.rb")
                .unwrap()
                .as_ref()
                .map(|t| t.name.as_str()),
            None
        );
    }
}
