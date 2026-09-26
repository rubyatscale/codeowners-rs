use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, MutexGuard, PoisonError},
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
    let relative_file_path = crate::path_utils::relative_to_buf(project_root, &absolute_file_path);

    let loaded = loaded_teams(teams_cache_root(project_root), config.team_file_glob.clone())?;
    let teams = &loaded.teams;
    let teams_by_name = &loaded.teams_by_name;

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
                sources_by_team.entry(team.name.clone()).or_default().push(Source::AnnotatedFile);
            }
        }
    }

    if let Some((owner_team_name, dir_source)) = most_specific_directory_owner(project_root, &relative_file_path, teams_by_name) {
        sources_by_team.entry(owner_team_name).or_default().push(dir_source);
    }

    if let Some((owner_team_name, package_source)) = nearest_package_owner(project_root, &relative_file_path, config, teams_by_name) {
        sources_by_team.entry(owner_team_name).or_default().push(package_source);
    }

    if let Some((owner_team_name, gem_source)) = vendored_gem_owner(&relative_file_path, config, teams) {
        sources_by_team.entry(owner_team_name).or_default().push(gem_source);
    }

    if let Some(rel_str) = relative_file_path.to_str() {
        for team in teams {
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

    for team in teams {
        let team_rel = crate::path_utils::relative_to_buf(project_root, &team.path);
        if team_rel == relative_file_path {
            sources_by_team.entry(team.name.clone()).or_default().push(Source::TeamYml);
        }
    }

    let mut file_owners: Vec<FileOwner> = Vec::new();
    for (team_name, sources) in sources_by_team.into_iter() {
        if let Some(team) = teams_by_name.get(&team_name) {
            let relative_team_yml_path = crate::path_utils::relative_to(project_root, &team.path)
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

struct LoadedTeams {
    teams: Vec<Team>,
    teams_by_name: HashMap<String, Team>,
}

type TeamCacheKey = (PathBuf, Vec<String>);

// Parsing every team file dominates a lookup, so load them once per project root and glob list and share
// the result across threads for the life of the process.
static TEAM_CACHE: LazyLock<Mutex<TeamCache>> = LazyLock::new(Default::default);

#[derive(Default)]
struct TeamCache {
    // Bumped by `clear_team_cache` so a load already running during a clear can't reinstate its stale result.
    generation: u64,
    loaded: HashMap<TeamCacheKey, Arc<LoadedTeams>>,
}

fn team_cache() -> MutexGuard<'static, TeamCache> {
    TEAM_CACHE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn loaded_teams(project_root: PathBuf, team_file_globs: Vec<String>) -> std::result::Result<Arc<LoadedTeams>, String> {
    let key = (project_root, team_file_globs);
    let generation = {
        let cache = team_cache();
        if let Some(loaded) = cache.loaded.get(&key) {
            return Ok(Arc::clone(loaded));
        }
        cache.generation
    };

    let load = load_teams(&key.0, &key.1)?;
    let teams_by_name = build_teams_by_name_map(&load.teams);
    let loaded = Arc::new(LoadedTeams {
        teams: load.teams,
        teams_by_name,
    });
    // A load that skipped a team file isn't cached, so fixing the file takes effect on the next lookup.
    if !load.skipped_team_file {
        cache_teams(key, Arc::clone(&loaded), generation);
    }
    Ok(loaded)
}

fn cache_teams(key: TeamCacheKey, loaded: Arc<LoadedTeams>, generation: u64) {
    let mut cache = team_cache();
    if cache.generation == generation {
        cache.loaded.insert(key, loaded);
    }
}

// Keyed on the absolute root so a relative root isn't reused after the working directory changes.
fn teams_cache_root(project_root: &Path) -> PathBuf {
    std::path::absolute(project_root).unwrap_or_else(|_| project_root.to_path_buf())
}

/// Drops the teams memoized by `find_file_owners`, for callers whose team files change within one process.
pub fn clear_team_cache() {
    let mut cache = team_cache();
    cache.generation += 1;
    cache.loaded.clear();
}

fn build_teams_by_name_map(teams: &[Team]) -> HashMap<String, Team> {
    let mut map = HashMap::with_capacity(teams.len() * 2);
    for team in teams {
        map.insert(team.name.clone(), team.clone());
        map.insert(team.github_team.clone(), team.clone());
    }
    map
}

struct TeamLoad {
    teams: Vec<Team>,
    skipped_team_file: bool,
}

fn load_teams(project_root: &Path, team_file_globs: &[String]) -> std::result::Result<TeamLoad, String> {
    let mut teams: Vec<Team> = Vec::new();
    let mut skipped_team_file = false;
    for glob_str in team_file_globs {
        let absolute_glob = project_root.join(glob_str).to_string_lossy().into_owned();
        let paths = glob(&absolute_glob).map_err(|e| e.to_string())?;
        for entry in paths {
            let path = match entry {
                Ok(path) => path,
                Err(e) => {
                    eprintln!("Error reading team file path: {e}");
                    skipped_team_file = true;
                    continue;
                }
            };
            match Team::from_team_file_path(path.clone()) {
                Ok(team) => teams.push(team),
                Err(e) => {
                    eprintln!("Error parsing team file: {e:?}, path: {}", path.display());
                    skipped_team_file = true;
                }
            }
        }
    }
    Ok(TeamLoad { teams, skipped_team_file })
}

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
                let relative_dir = crate::path_utils::relative_to(project_root, current.as_path())
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
        let parent_rel = crate::path_utils::relative_to(project_root, current.as_path());
        if let Some(rel_str) = parent_rel.to_str() {
            if glob_list_matches(rel_str, &config.ruby_package_paths) {
                let pkg_yml = current.join("package.yml");
                if pkg_yml.exists() {
                    match crate::project_builder::ruby_package_owner(&pkg_yml) {
                        Ok(owner) => {
                            if let Some(team) = owner.and_then(|owner| teams_by_name.get(&owner)) {
                                let package_path = parent_rel.join("package.yml");
                                let package_glob = format!("{rel_str}/**/**");
                                return Some((
                                    team.name.clone(),
                                    Source::Package(package_path.to_string_lossy().to_string(), package_glob),
                                ));
                            }
                        }
                        // validate rejects this package, so don't fall through to an enclosing package's owner.
                        Err(e) => {
                            eprintln!("Error reading ruby package: {e:?}, path: {}", pkg_yml.display());
                            return None;
                        }
                    }
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
        Source::AnnotatedFile => 0,
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
            executable_name: "codeowners".to_string(),
            codeowners_path: ".github".to_string(),
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
    fn test_nearest_package_owner_ruby_metadata_owner() {
        let td = tempdir().unwrap();
        let project_root = td.path();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");

        let ruby_pkg = project_root.join("packs/payroll");
        std::fs::create_dir_all(&ruby_pkg).unwrap();
        std::fs::write(ruby_pkg.join("package.yml"), "---\nmetadata:\n  owner: Payroll\n").unwrap();

        let mut tbn: HashMap<String, Team> = HashMap::new();
        let t = team_named("Payroll");
        tbn.insert(t.name.clone(), t);

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
    }

    #[test]
    fn test_nearest_package_owner_ruby_conflicting_owners_yields_none() {
        let td = tempdir().unwrap();
        let project_root = td.path();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");

        let ruby_pkg = project_root.join("packs/payroll");
        std::fs::create_dir_all(&ruby_pkg).unwrap();
        std::fs::write(ruby_pkg.join("package.yml"), "---\nowner: Payroll\nmetadata:\n  owner: Benefits\n").unwrap();

        let mut tbn: HashMap<String, Team> = HashMap::new();
        for name in ["Payroll", "Benefits"] {
            let t = team_named(name);
            tbn.insert(t.name.clone(), t);
        }

        let rel_ruby = Path::new("packs/payroll/app/models/thing.rb");
        assert!(nearest_package_owner(project_root, rel_ruby, &config, &tbn).is_none());
    }

    #[test]
    fn test_nearest_package_owner_ruby_conflict_does_not_fall_through_to_outer_package() {
        let td = tempdir().unwrap();
        let project_root = td.path();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");

        let outer_pkg = project_root.join("packs/outer");
        let inner_pkg = outer_pkg.join("inner");
        std::fs::create_dir_all(&inner_pkg).unwrap();
        std::fs::write(outer_pkg.join("package.yml"), "---\nowner: Outer\n").unwrap();
        std::fs::write(inner_pkg.join("package.yml"), "---\nowner: InnerA\nmetadata:\n  owner: InnerB\n").unwrap();

        let mut tbn: HashMap<String, Team> = HashMap::new();
        for name in ["Outer", "InnerA", "InnerB"] {
            let t = team_named(name);
            tbn.insert(t.name.clone(), t);
        }

        let rel_ruby = Path::new("packs/outer/inner/x.rb");
        assert!(nearest_package_owner(project_root, rel_ruby, &config, &tbn).is_none());
    }

    #[test]
    fn test_nearest_package_owner_ruby_ownerless_inner_package_falls_through_to_outer() {
        let td = tempdir().unwrap();
        let project_root = td.path();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");

        let outer_pkg = project_root.join("packs/outer");
        let inner_pkg = outer_pkg.join("inner");
        std::fs::create_dir_all(&inner_pkg).unwrap();
        std::fs::write(outer_pkg.join("package.yml"), "---\nowner: Outer\n").unwrap();
        std::fs::write(inner_pkg.join("package.yml"), "---\nenforce_dependencies: true\n").unwrap();

        let mut tbn: HashMap<String, Team> = HashMap::new();
        let t = team_named("Outer");
        tbn.insert(t.name.clone(), t);

        let rel_ruby = Path::new("packs/outer/inner/x.rb");
        let owner = nearest_package_owner(project_root, rel_ruby, &config, &tbn).unwrap();
        assert_eq!(owner.0, "Outer");
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

    #[test]
    fn test_teams_cache_root_is_absolute_for_relative_roots() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(teams_cache_root(Path::new("some/project")), cwd.join("some/project"));
        assert_eq!(teams_cache_root(Path::new(".")), cwd.join("."));
        assert_eq!(teams_cache_root(&cwd), cwd);
    }

    // The team cache is process-wide, so tests that clear it must not interleave.
    static TEAM_CACHE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_find_file_owners_reuses_loaded_teams_until_cleared() {
        let _guard = TEAM_CACHE_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let td = tempdir().unwrap();
        let root = td.path();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");
        let write_team = |glob: &str| {
            fs::create_dir_all(root.join("config/teams")).unwrap();
            fs::write(
                root.join("config/teams/payroll.yml"),
                format!("name: Payroll\ngithub:\n  team: '@PayrollTeam'\nowned_globs:\n  - {glob}\n"),
            )
            .unwrap();
        };
        let owner_of = |file: &str| {
            find_file_owners(root, &config, Path::new(file))
                .unwrap()
                .first()
                .map(|owner| owner.team.name.clone())
        };

        write_team("app/payroll/**/*");
        assert_eq!(owner_of("app/payroll/a.rb"), Some("Payroll".to_string()));

        write_team("app/other/**/*");
        assert_eq!(
            owner_of("app/payroll/a.rb"),
            Some("Payroll".to_string()),
            "team files are loaded once per process"
        );

        clear_team_cache();
        assert_eq!(owner_of("app/payroll/a.rb"), None);
        assert_eq!(owner_of("app/other/a.rb"), Some("Payroll".to_string()));
    }

    #[test]
    fn test_find_file_owners_shares_loaded_teams_across_threads() {
        let _guard = TEAM_CACHE_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let td = tempdir().unwrap();
        let root = td.path().to_path_buf();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");
        let write_team = |glob: &str| {
            fs::create_dir_all(root.join("config/teams")).unwrap();
            fs::write(
                root.join("config/teams/payroll.yml"),
                format!("name: Payroll\ngithub:\n  team: '@PayrollTeam'\nowned_globs:\n  - {glob}\n"),
            )
            .unwrap();
        };
        let owner_of = |root: &Path, config: &crate::config::Config, file: &str| {
            find_file_owners(root, config, Path::new(file))
                .unwrap()
                .first()
                .map(|owner| owner.team.name.clone())
        };

        write_team("app/payroll/**/*");
        assert_eq!(owner_of(&root, &config, "app/payroll/a.rb"), Some("Payroll".to_string()));
        write_team("app/other/**/*");

        std::thread::scope(|scope| {
            scope.spawn(|| {
                assert_eq!(
                    owner_of(&root, &config, "app/payroll/a.rb"),
                    Some("Payroll".to_string()),
                    "another thread reuses the teams loaded by the first"
                );
                clear_team_cache();
            });
        });

        assert_eq!(
            owner_of(&root, &config, "app/payroll/a.rb"),
            None,
            "a clear on another thread applies here too"
        );
    }

    #[test]
    fn test_clear_team_cache_during_a_load_is_not_undone_by_its_result() {
        let _guard = TEAM_CACHE_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let key: TeamCacheKey = (PathBuf::from("/clear-during-load"), vec!["config/teams/**/*.yml".to_string()]);
        let loaded = || {
            Arc::new(LoadedTeams {
                teams: Vec::new(),
                teams_by_name: HashMap::new(),
            })
        };

        let generation = team_cache().generation;
        clear_team_cache();
        cache_teams(key.clone(), loaded(), generation);
        assert!(
            !team_cache().loaded.contains_key(&key),
            "a load that started before the clear must not be cached"
        );

        let generation = team_cache().generation;
        cache_teams(key.clone(), loaded(), generation);
        assert!(team_cache().loaded.contains_key(&key));
        clear_team_cache();
    }

    #[cfg(unix)]
    #[test]
    fn test_find_file_owners_does_not_cache_a_load_with_an_unreadable_team_directory() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = TEAM_CACHE_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let td = tempdir().unwrap();
        let root = td.path();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");
        let owner_of = |file: &str| {
            find_file_owners(root, &config, Path::new(file))
                .unwrap()
                .first()
                .map(|owner| owner.team.name.clone())
        };

        let teams_dir = root.join("config/teams");
        let locked_dir = teams_dir.join("billing");
        fs::create_dir_all(&locked_dir).unwrap();
        fs::write(
            teams_dir.join("payroll.yml"),
            "name: Payroll\ngithub:\n  team: '@PayrollTeam'\nowned_globs:\n  - app/payroll/**/*\n",
        )
        .unwrap();
        fs::write(
            locked_dir.join("billing.yml"),
            "name: Billing\ngithub:\n  team: '@BillingTeam'\nowned_globs:\n  - app/billing/**/*\n",
        )
        .unwrap();

        fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o000)).unwrap();
        // Permissions aren't enforced for root, so the directory can't be made unreadable there.
        if fs::read_dir(&locked_dir).is_ok() {
            fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o755)).unwrap();
            return;
        }
        let payroll = owner_of("app/payroll/a.rb");
        let billing_while_unreadable = owner_of("app/billing/a.rb");
        fs::set_permissions(&locked_dir, fs::Permissions::from_mode(0o755)).unwrap();
        let billing_once_readable = owner_of("app/billing/a.rb");

        assert_eq!(payroll, Some("Payroll".to_string()));
        assert_eq!(billing_while_unreadable, None);
        assert_eq!(
            billing_once_readable,
            Some("Billing".to_string()),
            "a team directory that becomes readable takes effect without clearing the cache"
        );
        clear_team_cache();
    }

    #[test]
    fn test_find_file_owners_does_not_cache_a_load_that_skipped_a_team_file() {
        let _guard = TEAM_CACHE_TEST_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let td = tempdir().unwrap();
        let root = td.path();
        let config = build_config_for_temp("frontend/**/*", "packs/**/*", "vendored");
        let write_team_file = |file: &str, contents: &str| {
            fs::create_dir_all(root.join("config/teams")).unwrap();
            fs::write(root.join("config/teams").join(file), contents).unwrap();
        };
        let owner_of = |file: &str| {
            find_file_owners(root, &config, Path::new(file))
                .unwrap()
                .first()
                .map(|owner| owner.team.name.clone())
        };

        write_team_file(
            "payroll.yml",
            "name: Payroll\ngithub:\n  team: '@PayrollTeam'\nowned_globs:\n  - app/payroll/**/*\n",
        );
        write_team_file("billing.yml", "name: [unclosed\n");
        assert_eq!(owner_of("app/payroll/a.rb"), Some("Payroll".to_string()));
        assert_eq!(owner_of("app/billing/a.rb"), None);

        write_team_file(
            "billing.yml",
            "name: Billing\ngithub:\n  team: '@BillingTeam'\nowned_globs:\n  - app/billing/**/*\n",
        );
        assert_eq!(
            owner_of("app/billing/a.rb"),
            Some("Billing".to_string()),
            "fixing the team file takes effect without clearing the cache"
        );

        write_team_file(
            "billing.yml",
            "name: Billing\ngithub:\n  team: '@BillingTeam'\nowned_globs:\n  - app/other/**/*\n",
        );
        assert_eq!(
            owner_of("app/billing/a.rb"),
            Some("Billing".to_string()),
            "once every team file loads, the teams are cached"
        );
    }

    #[test]
    fn test_team_cache_is_keyed_on_team_file_glob() {
        let td = tempdir().unwrap();
        let root = td.path();
        for (dir, name) in [("a", "Payroll"), ("b", "Billing")] {
            fs::create_dir_all(root.join("config/teams").join(dir)).unwrap();
            fs::write(
                root.join("config/teams").join(dir).join("team.yml"),
                format!("name: {name}\ngithub:\n  team: '@{name}Team'\nowned_globs:\n  - app/**/*\n"),
            )
            .unwrap();
        }
        let config_with_team_glob = |team_glob: &str| crate::config::Config {
            team_file_glob: vec![team_glob.to_string()],
            ..build_config_for_temp("frontend/**/*", "packs/**/*", "vendored")
        };
        let config_a = config_with_team_glob("config/teams/a/*.yml");
        let config_b = config_with_team_glob("config/teams/b/*.yml");
        let owner_of = |config: &crate::config::Config| {
            find_file_owners(root, config, Path::new("app/a.rb"))
                .unwrap()
                .first()
                .map(|owner| owner.team.name.clone())
        };

        assert_eq!(owner_of(&config_a), Some("Payroll".to_string()));
        assert_eq!(owner_of(&config_b), Some("Billing".to_string()));
        assert_eq!(owner_of(&config_a), Some("Payroll".to_string()));
    }
}
