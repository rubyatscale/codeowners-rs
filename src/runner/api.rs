use std::collections::HashMap;
use std::path::Path;

use crate::ownership::FileOwner;
use crate::project::Team;

use super::{Error, RunConfig, RunResult, Runner, config_from_path, run};

pub fn for_file(run_config: &RunConfig, file_path: &str, from_codeowners: bool) -> RunResult {
    run(run_config, |runner| {
        if from_codeowners {
            runner.for_file_codeowners_only(file_path)
        } else {
            runner.for_file_optimized(file_path)
        }
    })
}

pub fn for_team(run_config: &RunConfig, team_name: &str) -> RunResult {
    run(run_config, |runner| runner.for_team(team_name))
}

pub fn validate(run_config: &RunConfig, _file_paths: Vec<String>) -> RunResult {
    run(run_config, |runner| runner.validate())
}

pub fn generate(run_config: &RunConfig, git_stage: bool) -> RunResult {
    run(run_config, |runner| runner.generate(git_stage))
}

pub fn generate_and_validate(run_config: &RunConfig, _file_paths: Vec<String>, git_stage: bool) -> RunResult {
    run(run_config, |runner| runner.generate_and_validate(git_stage))
}

pub fn delete_cache(run_config: &RunConfig) -> RunResult {
    run(run_config, |runner| runner.delete_cache())
}

pub fn crosscheck_owners(run_config: &RunConfig) -> RunResult {
    run(run_config, |runner| runner.crosscheck_owners())
}

// Returns the highest priority owner for a file. More to come here.
pub fn file_owner_for_file(run_config: &RunConfig, file_path: &str) -> error_stack::Result<Option<FileOwner>, Error> {
    let runner = Runner::new(run_config)?;
    let owners = runner.owners_for_file(file_path)?;
    Ok(owners.first().cloned())
}

pub fn team_for_file(run_config: &RunConfig, file_path: &str) -> error_stack::Result<Option<Team>, Error> {
    let owner = file_owner_for_file(run_config, file_path)?;
    Ok(owner.map(|fo| fo.team.clone()))
}

// For an array of file paths, return a map of file path to its owning team
pub fn teams_for_files_from_codeowners(
    run_config: &RunConfig,
    file_paths: &[String],
) -> error_stack::Result<HashMap<String, Option<Team>>, Error> {
    let config = config_from_path(&run_config.config_path)?;
    let res = crate::ownership::codeowners_query::teams_for_files_from_codeowners(
        &run_config.project_root,
        &run_config.codeowners_file_path,
        &config.team_file_glob,
        file_paths,
    )
    .map_err(Error::Io)?;
    Ok(res)
}

pub fn team_for_file_from_codeowners(run_config: &RunConfig, file_path: &str) -> error_stack::Result<Option<Team>, Error> {
    let relative_file_path = crate::path_utils::relative_to(&run_config.project_root, Path::new(file_path));

    let config = config_from_path(&run_config.config_path)?;
    let res = crate::ownership::codeowners_query::team_for_file_from_codeowners(
        &run_config.project_root,
        &run_config.codeowners_file_path,
        &config.team_file_glob,
        Path::new(relative_file_path),
    )
    .map_err(Error::Io)?;
    Ok(res)
}
