use std::collections::HashMap;
use std::path::Path;

use crate::ownership::FileOwner;
use crate::project::Team;

use super::{Error, ForFileResult, RunConfig, RunResult, config_from_path, run};

pub fn for_file(run_config: &RunConfig, file_path: &str, from_codeowners: bool, json: bool) -> RunResult {
    if from_codeowners {
        return for_file_codeowners_only_fast(run_config, file_path, json);
    }
    for_file_optimized(run_config, file_path, json)
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

// Returns all owners for a file without creating a Runner (performance optimized)
pub fn owners_for_file(run_config: &RunConfig, file_path: &str) -> error_stack::Result<Vec<FileOwner>, Error> {
    let config = config_from_path(&run_config.config_path)?;
    use crate::ownership::file_owner_resolver::find_file_owners;
    let owners = find_file_owners(&run_config.project_root, &config, std::path::Path::new(file_path)).map_err(Error::Io)?;
    Ok(owners)
}

// Returns the highest priority owner for a file. More to come here.
pub fn file_owner_for_file(run_config: &RunConfig, file_path: &str) -> error_stack::Result<Option<FileOwner>, Error> {
    let owners = owners_for_file(run_config, file_path)?;
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

// Fast path that avoids creating a full Runner for single file queries
fn for_file_optimized(run_config: &RunConfig, file_path: &str, json: bool) -> RunResult {
    let config = match config_from_path(&run_config.config_path) {
        Ok(c) => c,
        Err(err) => {
            return RunResult::from_io_error(Error::Io(err.to_string()), json);
        }
    };

    use crate::ownership::file_owner_resolver::find_file_owners;
    let file_owners = match find_file_owners(&run_config.project_root, &config, std::path::Path::new(file_path)) {
        Ok(v) => v,
        Err(err) => {
            return RunResult::from_io_error(Error::Io(err), json);
        }
    };

    match file_owners.as_slice() {
        [] => RunResult::from_file_owner(&crate::ownership::FileOwner::default(), json),
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

fn for_file_codeowners_only_fast(run_config: &RunConfig, file_path: &str, json: bool) -> RunResult {
    match team_for_file_from_codeowners(run_config, file_path) {
        Ok(Some(team)) => {
            let team_yml = crate::path_utils::relative_to(&run_config.project_root, team.path.as_path())
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
        Ok(None) => RunResult::from_file_owner(&crate::ownership::FileOwner::default(), json),
        Err(err) => RunResult::from_io_error(Error::Io(format!("{}", err)), json),
    }
}
