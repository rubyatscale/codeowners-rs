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
        for_file_optimized(run_config, file_path)
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

use std::collections::{HashMap, HashSet};
use std::fs;
use fast_glob::glob_match;
use glob::glob;
use lazy_static::lazy_static;
use regex::Regex;

fn for_file_optimized(run_config: &RunConfig, file_path: &str) -> RunResult {
    let config = match config_from_path(&run_config.config_path) {
        Ok(c) => c,
        Err(err) => {
            return RunResult {
                io_errors: vec![err.to_string()],
                ..Default::default()
            }
        }
    };

    let absolute_file_path = std::path::Path::new(file_path);
    let relative_file_path = absolute_file_path
        .strip_prefix(&run_config.project_root)
        .unwrap_or(absolute_file_path)
        .to_path_buf();

    let teams = match load_teams(&run_config.project_root, &config.team_file_glob) {
        Ok(t) => t,
        Err(err) => {
            return RunResult {
                io_errors: vec![err.to_string()],
                ..Default::default()
            }
        }
    };
    let teams_by_name = build_teams_by_name_map(&teams);

    let mut sources_by_team: HashMap<String, Vec<crate::ownership::mapper::Source>> = HashMap::new();

    if let Some(team_name) = read_top_of_file_team(&absolute_file_path.to_path_buf()) {
        if let Some(team) = teams_by_name.get(&team_name) {
            sources_by_team.entry(team.name.clone()).or_default().push(crate::ownership::mapper::Source::TeamFile);
        }
    }

    if let Some((owner_team_name, dir_source)) = most_specific_directory_owner(
        &run_config.project_root,
        &relative_file_path,
        &teams_by_name,
    ) {
        sources_by_team.entry(owner_team_name).or_default().push(dir_source);
    }

    if let Some((owner_team_name, package_source)) = nearest_package_owner(
        &run_config.project_root,
        &relative_file_path,
        &config,
        &teams_by_name,
    ) {
        sources_by_team.entry(owner_team_name).or_default().push(package_source);
    }

    if let Some((owner_team_name, gem_source)) = vendored_gem_owner(&relative_file_path, &config, &teams) {
        sources_by_team.entry(owner_team_name).or_default().push(gem_source);
    }

    if let Some(rel_str) = relative_file_path.to_str() {
        for team in &teams {
            let subtracts: HashSet<&str> = team.subtracted_globs.iter().map(|s| s.as_str()).collect();
            for owned_glob in &team.owned_globs {
                if glob_match(owned_glob, rel_str) && !subtracts.iter().any(|sub| glob_match(sub, rel_str)) {
                    sources_by_team
                        .entry(team.name.clone())
                        .or_default()
                        .push(crate::ownership::mapper::Source::TeamGlob(owned_glob.clone()));
                }
            }
        }
    }

    for team in &teams {
        let team_rel = team
            .path
            .strip_prefix(&run_config.project_root)
            .unwrap_or(&team.path)
            .to_path_buf();
        if team_rel == relative_file_path {
            sources_by_team.entry(team.name.clone()).or_default().push(crate::ownership::mapper::Source::TeamYml);
        }
    }

    let mut file_owners: Vec<FileOwner> = Vec::new();
    for (team_name, sources) in sources_by_team.into_iter() {
        if let Some(team) = teams_by_name.get(&team_name) {
            let relative_team_yml_path = team
                .path
                .strip_prefix(&run_config.project_root)
                .unwrap_or(&team.path)
                .to_string_lossy()
                .to_string();
            file_owners.push(FileOwner {
                team: team.clone(),
                team_config_file_path: relative_team_yml_path,
                sources,
            });
        }
    }

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
    RunResult { info_messages, ..Default::default() }
}

fn build_teams_by_name_map(teams: &[Team]) -> HashMap<String, Team> {
    let mut map = HashMap::new();
    for team in teams {
        map.insert(team.name.clone(), team.clone());
        map.insert(team.github_team.clone(), team.clone());
    }
    map
}

fn load_teams(project_root: &std::path::Path, team_file_globs: &[String]) -> std::result::Result<Vec<Team>, String> {
    let mut teams: Vec<Team> = Vec::new();
    for glob_str in team_file_globs {
        let absolute_glob = format!("{}/{}", project_root.display(), glob_str);
        let paths = glob(&absolute_glob).map_err(|e| e.to_string())?;
        for path in paths.flatten() {
            match Team::from_team_file_path(path.clone()) {
                Ok(team) => teams.push(team),
                Err(e) => {
                    eprintln!("Error parsing team file: {}", e);
                    continue;
                }
            }
        }
    }
    Ok(teams)
}

lazy_static! {
    static ref TOP_OF_FILE_TEAM_REGEX: Regex = Regex::new(r#"^(?:#|//) @team (.*)$"#).expect("error compiling regular expression");
}

fn read_top_of_file_team(path: &std::path::PathBuf) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let first_line = content.lines().next()?;
    TOP_OF_FILE_TEAM_REGEX
        .captures(first_line)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
}

fn most_specific_directory_owner(
    project_root: &std::path::Path,
    relative_file_path: &std::path::Path,
    teams_by_name: &HashMap<String, Team>,
) -> Option<(String, crate::ownership::mapper::Source)> {
    let mut current = project_root.join(relative_file_path);
    let mut best: Option<(String, crate::ownership::mapper::Source)> = None;
    loop {
        let parent_opt = current.parent().map(|p| p.to_path_buf());
        let Some(parent) = parent_opt else { break };
        let codeowner_path = parent.join(".codeowner");
        if let Ok(owner_str) = fs::read_to_string(&codeowner_path) {
            let owner = owner_str.trim();
            if let Some(team) = teams_by_name.get(owner) {
                let relative_dir = parent
                    .strip_prefix(project_root)
                    .unwrap_or(parent.as_path())
                    .to_string_lossy()
                    .to_string();
                let candidate = (
                    team.name.clone(),
                    crate::ownership::mapper::Source::Directory(relative_dir),
                );
                match &best {
                    None => best = Some(candidate),
                    Some((_, existing_source)) => {
                        let existing_len = source_directory_depth(existing_source);
                        let candidate_len = source_directory_depth(&candidate.1);
                        if candidate_len > existing_len {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }
        if parent == project_root { break; }
        current = parent.clone();
    }
    best
}

fn nearest_package_owner(
    project_root: &std::path::Path,
    relative_file_path: &std::path::Path,
    config: &Config,
    teams_by_name: &HashMap<String, Team>,
) -> Option<(String, crate::ownership::mapper::Source)> {
    let mut current = project_root.join(relative_file_path);
    loop {
        let parent_opt = current.parent().map(|p| p.to_path_buf());
        let Some(parent) = parent_opt else { break };
        let parent_rel = parent.strip_prefix(project_root).unwrap_or(parent.as_path());
        if let Some(rel_str) = parent_rel.to_str() {
            if glob_list_matches(rel_str, &config.ruby_package_paths) {
                let pkg_yml = parent.join("package.yml");
                if pkg_yml.exists() {
                    if let Ok(owner) = read_ruby_package_owner(&pkg_yml) {
                        if let Some(team) = teams_by_name.get(&owner) {
                            let package_path = parent_rel.join("package.yml");
                            let package_glob = format!("{}/**/**", rel_str);
                            return Some((
                                team.name.clone(),
                                crate::ownership::mapper::Source::Package(
                                    package_path.to_string_lossy().to_string(),
                                    package_glob,
                                ),
                            ));
                        }
                    }
                }
            }
            if glob_list_matches(rel_str, &config.javascript_package_paths) {
                let pkg_json = parent.join("package.json");
                if pkg_json.exists() {
                    if let Ok(owner) = read_js_package_owner(&pkg_json) {
                        if let Some(team) = teams_by_name.get(&owner) {
                            let package_path = parent_rel.join("package.json");
                            let package_glob = format!("{}/**/**", rel_str);
                            return Some((
                                team.name.clone(),
                                crate::ownership::mapper::Source::Package(
                                    package_path.to_string_lossy().to_string(),
                                    package_glob,
                                ),
                            ));
                        }
                    }
                }
            }
        }
        if parent == project_root { break; }
        current = parent;
    }
    None
}

fn source_directory_depth(source: &crate::ownership::mapper::Source) -> usize {
    match source {
        crate::ownership::mapper::Source::Directory(path) => path.matches('/').count(),
        _ => 0,
    }
}

fn glob_list_matches(path: &str, globs: &[String]) -> bool {
    globs.iter().any(|g| glob_match(g, path))
}

fn read_ruby_package_owner(path: &std::path::Path) -> std::result::Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let deserializer: crate::project::deserializers::RubyPackage = serde_yaml::from_reader(file).map_err(|e| e.to_string())?;
    deserializer.owner.ok_or_else(|| "Missing owner".to_string())
}

fn read_js_package_owner(path: &std::path::Path) -> std::result::Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let deserializer: crate::project::deserializers::JavascriptPackage = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    deserializer
        .metadata
        .and_then(|m| m.owner)
        .ok_or_else(|| "Missing owner".to_string())
}

fn vendored_gem_owner(
    relative_file_path: &std::path::Path,
    config: &Config,
    teams: &[Team],
) -> Option<(String, crate::ownership::mapper::Source)> {
    use std::path::Component;
    let mut comps = relative_file_path.components();
    let first = comps.next()?;
    let second = comps.next()?;
    let first_str = match first { Component::Normal(s) => s.to_string_lossy(), _ => return None };
    if first_str != config.vendored_gems_path { return None; }
    let gem_name = match second { Component::Normal(s) => s.to_string_lossy().to_string(), _ => return None };
    for team in teams {
        if team.owned_gems.iter().any(|g| g == &gem_name) {
            return Some((team.name.clone(), crate::ownership::mapper::Source::TeamGem));
        }
    }
    None
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
        let relative_file_path = Path::new(file_path)
            .strip_prefix(&self.run_config.project_root)
            .unwrap_or(Path::new(file_path));
        let file_owners = match self.ownership.for_file(relative_file_path) {
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
