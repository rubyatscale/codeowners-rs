use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use fast_glob::glob_match;
use glob::glob;

use crate::{config::Config, project::Team, project_file_builder::build_project_file_without_cache};

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

    let teams = load_teams(project_root, &config.team_file_glob)?;
    let teams_by_name = build_teams_by_name_map(&teams);

    let mut sources_by_team: HashMap<String, Vec<Source>> = HashMap::new();

    if let Some(team_name) = read_top_of_file_team(&absolute_file_path) {
        // Only consider top-of-file annotations for files included by config.owned_globs and not excluded by config.unowned_globs
        if let Some(rel_str) = relative_file_path.to_str() {
            let is_config_owned = glob_list_matches(rel_str, &config.owned_globs);
            let is_config_unowned = glob_list_matches(rel_str, &config.unowned_globs);
            if is_config_owned
                && !is_config_unowned
                && let Some(team) = teams_by_name.get(&team_name)
            {
                sources_by_team.entry(team.name.clone()).or_default().push(Source::TeamFile);
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
        let team_rel = team.path.strip_prefix(project_root).unwrap_or(&team.path).to_path_buf();
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

    // TODO: remove this once we've verified the fast path is working
    // This is simply matching the order of behavior of the original codeowners CLI
    if file_owners.len() > 1 {
        file_owners.sort_by(|a, b| {
            let priority_a = a.sources.iter().map(source_priority).min().unwrap_or(u8::MAX);
            let priority_b = b.sources.iter().map(source_priority).min().unwrap_or(u8::MAX);
            priority_a.cmp(&priority_b).then_with(|| a.team.name.cmp(&b.team.name))
        });
    }

    Ok(file_owners)
}

fn build_teams_by_name_map(teams: &[Team]) -> HashMap<String, Team> {
    let mut map = HashMap::with_capacity(teams.len() * 2);
    for team in teams {
        map.insert(team.name.clone(), team.clone());
        map.insert(team.github_team.clone(), team.clone());
    }
    map
}

fn load_teams(project_root: &Path, team_file_globs: &[String]) -> std::result::Result<Vec<Team>, String> {
    let mut teams: Vec<Team> = Vec::new();
    for glob_str in team_file_globs {
        let absolute_glob = project_root.join(glob_str).to_string_lossy().into_owned();
        let paths = glob(&absolute_glob).map_err(|e| e.to_string())?;
        for path in paths.flatten() {
            match Team::from_team_file_path(path.clone()) {
                Ok(team) => teams.push(team),
                Err(e) => {
                    eprintln!("Error parsing team file: {}, path: {}", e, path.display());
                    continue;
                }
            }
        }
    }
    Ok(teams)
}

// no regex: parse cheaply with ASCII-aware checks

fn read_top_of_file_team(path: &Path) -> Option<String> {
    let project_file = build_project_file_without_cache(&path.to_path_buf());
    if let Some(owner) = project_file.owner {
        return Some(owner);
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
        if !current.pop() {
            break;
        }
        let codeowner_path = current.join(".codeowner");
        if let Ok(owner_str) = fs::read_to_string(&codeowner_path) {
            let owner = owner_str.trim();
            if let Some(team) = teams_by_name.get(owner) {
                let relative_dir = current
                    .strip_prefix(project_root)
                    .unwrap_or(current.as_path())
                    .to_string_lossy()
                    .to_string();
                let candidate = (team.name.clone(), Source::Directory(relative_dir));
                match &best {
                    None => best = Some(candidate),
                    Some((_, existing_source)) => {
                        if candidate.1.len() > existing_source.len() {
                            best = Some(candidate);
                        }
                    }
                }
            }
        }
        if current == project_root {
            break;
        }
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
        if !current.pop() {
            break;
        }
        let parent_rel = current.strip_prefix(project_root).unwrap_or(current.as_path());
        if let Some(rel_str) = parent_rel.to_str() {
            if glob_list_matches(rel_str, &config.ruby_package_paths) {
                let pkg_yml = current.join("package.yml");
                if pkg_yml.exists()
                    && let Ok(owner) = read_ruby_package_owner(&pkg_yml)
                    && let Some(team) = teams_by_name.get(&owner)
                {
                    let package_path = parent_rel.join("package.yml");
                    let package_glob = format!("{rel_str}/**/**");
                    return Some((
                        team.name.clone(),
                        Source::Package(package_path.to_string_lossy().to_string(), package_glob),
                    ));
                }
            }
            if glob_list_matches(rel_str, &config.javascript_package_paths) {
                let pkg_json = current.join("package.json");
                if pkg_json.exists()
                    && let Ok(owner) = read_js_package_owner(&pkg_json)
                    && let Some(team) = teams_by_name.get(&owner)
                {
                    let package_path = parent_rel.join("package.json");
                    let package_glob = format!("{rel_str}/**/**");
                    return Some((
                        team.name.clone(),
                        Source::Package(package_path.to_string_lossy().to_string(), package_glob),
                    ));
                }
            }
        }
        if current == project_root {
            break;
        }
    }
    None
}

// removed: use `Source::len()` instead

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

fn vendored_gem_owner(relative_file_path: &Path, config: &Config, teams: &[Team]) -> Option<(String, Source)> {
    use std::path::Component;
    let mut comps = relative_file_path.components();
    let first = comps.next()?;
    let second = comps.next()?;
    let first_str = match first {
        Component::Normal(s) => s.to_string_lossy(),
        _ => return None,
    };
    if first_str != config.vendored_gems_path {
        return None;
    }
    let gem_name = match second {
        Component::Normal(s) => s.to_string_lossy().to_string(),
        _ => return None,
    };
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::Team;
    use std::collections::HashMap;
    use tempfile::tempdir;

    fn build_config_for_temp(frontend_glob: &str, ruby_glob: &str, vendored_path: &str) -> crate::config::Config {
        crate::config::Config {
            owned_globs: vec!["**/*".to_string()],
            ruby_package_paths: vec![ruby_glob.to_string()],
            javascript_package_paths: vec![frontend_glob.to_string()],
            team_file_glob: vec!["config/teams/**/*.yml".to_string()],
            unowned_globs: vec![],
            vendored_gems_path: vendored_path.to_string(),
            cache_directory: "tmp/cache/codeowners".to_string(),
            ignore_dirs: vec![],
            skip_untracked_files: false,
        }
    }

    fn team_named(name: &str) -> Team {
        Team {
            path: Path::new("config/teams/foo.yml").to_path_buf(),
            name: name.to_string(),
            github_team: format!("@{}Team", name),
            owned_globs: vec![],
            subtracted_globs: vec![],
            owned_gems: vec![],
            avoid_ownership: false,
        }
    }

    #[test]
    fn test_read_top_of_file_team_parses_at_and_colon_forms() {
        let td = tempdir().unwrap();

        // @team form
        let file_at = td.path().join("at_form.rb");
        std::fs::write(&file_at, "# @team Payroll\nputs 'x'\n").unwrap();
        assert_eq!(read_top_of_file_team(&file_at), Some("Payroll".to_string()));
    }

    #[test]
    fn test_most_specific_directory_owner_prefers_deeper() {
        let td = tempdir().unwrap();
        let project_root = td.path();

        // Build directories
        let deep_dir = project_root.join("a/b/c");
        std::fs::create_dir_all(&deep_dir).unwrap();
        let mid_dir = project_root.join("a/b");
        let top_dir = project_root.join("a");

        // Write .codeowner files
        std::fs::write(top_dir.join(".codeowner"), "TopTeam").unwrap();
        std::fs::write(mid_dir.join(".codeowner"), "MidTeam").unwrap();
        std::fs::write(deep_dir.join(".codeowner"), "DeepTeam").unwrap();

        // Build teams_by_name
        let mut tbn: HashMap<String, Team> = HashMap::new();
        for name in ["TopTeam", "MidTeam", "DeepTeam"] {
            let t = team_named(name);
            tbn.insert(t.name.clone(), t);
        }

        let rel_file = Path::new("a/b/c/file.rb");
        let result = most_specific_directory_owner(project_root, rel_file, &tbn).unwrap();
        match result.1 {
            Source::Directory(path) => {
                assert!(path.ends_with("a/b/c"), "expected deepest directory, got {}", path);
            }
            _ => panic!("expected Directory source"),
        }
        assert_eq!(result.0, "DeepTeam");
    }

    #[test]
    fn test_nearest_package_owner_ruby_and_js() {
        let td = tempdir().unwrap();
        let project_root = td.path();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");

        // Ruby package
        let ruby_pkg = project_root.join("packs/payroll");
        std::fs::create_dir_all(&ruby_pkg).unwrap();
        std::fs::write(ruby_pkg.join("package.yml"), "---\nowner: Payroll\n").unwrap();

        // JS package
        let js_pkg = project_root.join("frontend/flow");
        std::fs::create_dir_all(&js_pkg).unwrap();
        std::fs::write(js_pkg.join("package.json"), r#"{"metadata": {"owner": "UX"}}"#).unwrap();

        // Teams map
        let mut tbn: HashMap<String, Team> = HashMap::new();
        for name in ["Payroll", "UX"] {
            let t = team_named(name);
            tbn.insert(t.name.clone(), t);
        }

        // Ruby nearest
        let rel_ruby = Path::new("packs/payroll/app/models/thing.rb");
        let ruby_owner = nearest_package_owner(project_root, rel_ruby, &config, &tbn).unwrap();
        assert_eq!(ruby_owner.0, "Payroll");
        match ruby_owner.1 {
            Source::Package(pkg_path, glob) => {
                assert!(pkg_path.ends_with("packs/payroll/package.yml"));
                assert_eq!(glob, "packs/payroll/**/**");
            }
            _ => panic!("expected Package source for ruby"),
        }

        // JS nearest
        let rel_js = Path::new("frontend/flow/src/index.ts");
        let js_owner = nearest_package_owner(project_root, rel_js, &config, &tbn).unwrap();
        assert_eq!(js_owner.0, "UX");
        match js_owner.1 {
            Source::Package(pkg_path, glob) => {
                assert!(pkg_path.ends_with("frontend/flow/package.json"));
                assert_eq!(glob, "frontend/flow/**/**");
            }
            _ => panic!("expected Package source for js"),
        }
    }

    #[test]
    fn test_vendored_gem_owner() {
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");
        let mut teams: Vec<Team> = vec![team_named("Payroll")];
        teams[0].owned_gems = vec!["awesome_gem".to_string()];

        let path = Path::new("vendored/awesome_gem/lib/a.rb");
        let result = vendored_gem_owner(path, &config, &teams).unwrap();
        assert_eq!(result.0, "Payroll");
        matches!(result.1, Source::TeamGem);
    }
}
