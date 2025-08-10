use std::{collections::{HashMap, HashSet}, fs, path::Path};

use fast_glob::glob_match;
use glob::glob;
use lazy_static::lazy_static;
use regex::Regex;

use crate::{config::Config, project::Team};

use super::{FileOwner, mapper::Source};

pub fn find_file_owners(project_root: &Path, config: &Config, file_path: &Path) -> Result<Vec<FileOwner>, String> {
    let absolute_file_path = if file_path.is_absolute() {
        file_path.to_path_buf()
    } else {
        project_root.join(file_path)
    };
    let relative_file_path = absolute_file_path
        .strip_prefix(project_root)
        .unwrap_or(&absolute_file_path)
        .to_path_buf();

    let teams = match load_teams(project_root, &config.team_file_glob) {
        Ok(t) => t,
        Err(e) => return Err(e),
    };
    let teams_by_name = build_teams_by_name_map(&teams);

    let mut sources_by_team: HashMap<String, Vec<Source>> = HashMap::new();

    if let Some(team_name) = read_top_of_file_team(&absolute_file_path) {
        // Only consider top-of-file annotations for files included by config.owned_globs and not excluded by config.unowned_globs
        if let Some(rel_str) = relative_file_path.to_str() {
            let is_config_owned = glob_list_matches(rel_str, &config.owned_globs);
            let is_config_unowned = glob_list_matches(rel_str, &config.unowned_globs);
            if is_config_owned && !is_config_unowned {
                if let Some(team) = teams_by_name.get(&team_name) {
                    sources_by_team.entry(team.name.clone()).or_default().push(Source::TeamFile);
                }
            }
        }
    }

    if let Some((owner_team_name, dir_source)) = most_specific_directory_owner(project_root, &relative_file_path, &teams_by_name) {
        sources_by_team.entry(owner_team_name).or_default().push(dir_source);
    }

    if let Some((owner_team_name, package_source)) = nearest_package_owner(project_root, &relative_file_path, config, &teams_by_name) {
        sources_by_team.entry(owner_team_name).or_default().push(package_source);
    }

    if let Some((owner_team_name, gem_source)) = vendored_gem_owner(&relative_file_path, config, &teams) {
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
                        .push(Source::TeamGlob(owned_glob.clone()));
                }
            }
        }
    }

    for team in &teams {
        let team_rel = team
            .path
            .strip_prefix(project_root)
            .unwrap_or(&team.path)
            .to_path_buf();
        if team_rel == relative_file_path {
            sources_by_team.entry(team.name.clone()).or_default().push(Source::TeamYml);
        }
    }

    let mut file_owners: Vec<FileOwner> = Vec::new();
    for (team_name, sources) in sources_by_team.into_iter() {
        if let Some(team) = teams_by_name.get(&team_name) {
            let relative_team_yml_path = team
                .path
                .strip_prefix(project_root)
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

    if file_owners.len() > 1 {
        file_owners.sort_by(|a, b| {
            let priority_a = a
                .sources
                .iter()
                .map(|s| source_priority(s))
                .min()
                .unwrap_or(u8::MAX);
            let priority_b = b
                .sources
                .iter()
                .map(|s| source_priority(s))
                .min()
                .unwrap_or(u8::MAX);
            priority_a.cmp(&priority_b).then_with(|| a.team.name.cmp(&b.team.name))
        });
    }

    Ok(file_owners)
}

fn build_teams_by_name_map(teams: &[Team]) -> HashMap<String, Team> {
    let mut map = HashMap::new();
    for team in teams {
        map.insert(team.name.clone(), team.clone());
        map.insert(team.github_team.clone(), team.clone());
    }
    map
}

fn load_teams(project_root: &Path, team_file_globs: &[String]) -> std::result::Result<Vec<Team>, String> {
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
    // Allow optional leading whitespace before the comment marker
    static ref TOP_OF_FILE_TEAM_AT_REGEX: Option<Regex> = Regex::new(r#"^\s*(?:#|//)\s*@team\s+(.+)$"#).ok();
    static ref TOP_OF_FILE_TEAM_COLON_REGEX: Option<Regex> = Regex::new(r#"(?i)^\s*(?:#|//)\s*team\s*:\s*(.+)$"#).ok();
}

fn read_top_of_file_team(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let line = content.lines().next()?;

    if let Some(re) = &*TOP_OF_FILE_TEAM_AT_REGEX {
        if let Some(cap) = re.captures(line) {
            if let Some(m) = cap.get(1) {
                return Some(m.as_str().to_string());
            }
        }
    }
    if let Some(re) = &*TOP_OF_FILE_TEAM_COLON_REGEX {
        if let Some(cap) = re.captures(line) {
            if let Some(m) = cap.get(1) {
                return Some(m.as_str().to_string());
            }
        }
    }
    None
}

fn most_specific_directory_owner(
    project_root: &Path,
    relative_file_path: &Path,
    teams_by_name: &HashMap<String, Team>,
) -> Option<(String, Source)> {
    let mut current = project_root.join(relative_file_path);
    let mut best: Option<(String, Source)> = None;
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
                let candidate = (team.name.clone(), Source::Directory(relative_dir));
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
    project_root: &Path,
    relative_file_path: &Path,
    config: &Config,
    teams_by_name: &HashMap<String, Team>,
) -> Option<(String, Source)> {
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
                            return Some((team.name.clone(), Source::Package(
                                package_path.to_string_lossy().to_string(),
                                package_glob,
                            )));
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
                            return Some((team.name.clone(), Source::Package(
                                package_path.to_string_lossy().to_string(),
                                package_glob,
                            )));
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

fn source_directory_depth(source: &Source) -> usize {
    match source {
        Source::Directory(path) => path.matches('/').count(),
        _ => 0,
    }
}

fn glob_list_matches(path: &str, globs: &[String]) -> bool {
    globs.iter().any(|g| glob_match(g, path))
}

fn read_ruby_package_owner(path: &Path) -> std::result::Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let deserializer: crate::project::deserializers::RubyPackage = serde_yaml::from_reader(file).map_err(|e| e.to_string())?;
    deserializer.owner.ok_or_else(|| "Missing owner".to_string())
}

fn read_js_package_owner(path: &Path) -> std::result::Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let deserializer: crate::project::deserializers::JavascriptPackage = serde_json::from_reader(file).map_err(|e| e.to_string())?;
    deserializer
        .metadata
        .and_then(|m| m.owner)
        .ok_or_else(|| "Missing owner".to_string())
}

fn vendored_gem_owner(
    relative_file_path: &Path,
    config: &Config,
    teams: &[Team],
) -> Option<(String, Source)> {
    use std::path::Component;
    let mut comps = relative_file_path.components();
    let first = comps.next()?;
    let second = comps.next()?;
    let first_str = match first { Component::Normal(s) => s.to_string_lossy(), _ => return None };
    if first_str != config.vendored_gems_path { return None; }
    let gem_name = match second { Component::Normal(s) => s.to_string_lossy().to_string(), _ => return None };
    for team in teams {
        if team.owned_gems.iter().any(|g| g == &gem_name) {
            return Some((team.name.clone(), Source::TeamGem));
        }
    }
    None
}

fn source_priority(source: &Source) -> u8 {
    match source {
        // Highest confidence first
        Source::TeamFile => 0,
        Source::Directory(_) => 1,
        Source::Package(_, _) => 2,
        Source::TeamGlob(_) => 3,
        Source::TeamGem => 4,
        Source::TeamYml => 5,
    }
}


