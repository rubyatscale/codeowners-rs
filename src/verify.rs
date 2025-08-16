use std::path::Path;

use crate::{
    cache::Cache,
    config::Config,
    ownership::for_file_fast::find_file_owners,
    ownership::mapper::Source,
    project::Project,
    project_builder::ProjectBuilder,
    runner::{RunConfig, RunResult, config_from_path, team_for_file_from_codeowners},
};

pub fn verify_compare_for_file(run_config: &RunConfig, cache: &Cache) -> RunResult {
    match do_verify_compare_for_file(run_config, cache) {
        Ok(mismatches) if mismatches.is_empty() => RunResult {
            info_messages: vec!["Success! All files match between CODEOWNERS and for-file command.".to_string()],
            ..Default::default()
        },
        Ok(mismatches) => RunResult {
            validation_errors: mismatches,
            ..Default::default()
        },
        Err(err) => RunResult {
            io_errors: vec![err],
            ..Default::default()
        },
    }
}

fn do_verify_compare_for_file(run_config: &RunConfig, cache: &Cache) -> Result<Vec<String>, String> {
    let config = load_config(run_config)?;
    let project = build_project(&config, run_config, cache)?;

    let mut mismatches: Vec<String> = Vec::new();
    for file in &project.files {
        let (codeowners_team, fast_display) = owners_for_file(&file.path, run_config, &config)?;
        let codeowners_display = codeowners_team.clone().unwrap_or_else(|| "Unowned".to_string());
        if !is_match(codeowners_team.as_deref(), &fast_display) {
            mismatches.push(format_mismatch(&project, &file.path, &codeowners_display, &fast_display));
        }
    }

    Ok(mismatches)
}

fn load_config(run_config: &RunConfig) -> Result<Config, String> {
    config_from_path(&run_config.config_path).map_err(|e| e.to_string())
}

fn build_project(config: &Config, run_config: &RunConfig, cache: &Cache) -> Result<Project, String> {
    let mut project_builder = ProjectBuilder::new(
        config,
        run_config.project_root.clone(),
        run_config.codeowners_file_path.clone(),
        cache,
    );
    project_builder.build().map_err(|e| e.to_string())
}

fn owners_for_file(path: &Path, run_config: &RunConfig, config: &Config) -> Result<(Option<String>, String), String> {
    let file_path_str = path.to_string_lossy().to_string();

    let codeowners_team = team_for_file_from_codeowners(run_config, &file_path_str)
        .map_err(|e| e.to_string())?
        .map(|t| t.name);

    let fast_owners = find_file_owners(&run_config.project_root, config, Path::new(&file_path_str))?;

    // Determine highest-priority owner(s)
    let min_priority = |fo: &crate::ownership::FileOwner| -> u8 {
        fo.sources
            .iter()
            .map(|s| match s {
                Source::TeamFile => 0,
                Source::Directory(_) => 1,
                Source::Package(_, _) => 2,
                Source::TeamGlob(_) => 3,
                Source::TeamGem => 4,
                Source::TeamYml => 5,
            })
            .min()
            .unwrap_or(u8::MAX)
    };

    let fast_display = if fast_owners.is_empty() {
        "Unowned".to_string()
    } else {
        let top_priority = fast_owners.iter().map(min_priority).min().unwrap_or(u8::MAX);

        let winners: Vec<&crate::ownership::FileOwner> = fast_owners.iter().filter(|fo| min_priority(fo) == top_priority).collect();

        if winners.len() > 1 && top_priority == 3 {
            let names: Vec<String> = winners.into_iter().map(|fo| fo.team.name.clone()).collect();
            format!("Multiple: {}", names.join(", "))
        } else {
            let winner_name = fast_owners
                .iter()
                .min_by_key(|fo| (min_priority(fo), fo.team.name.clone()))
                .map(|fo| fo.team.name.clone())
                .unwrap_or_else(|| "Unowned".to_string());
            winner_name
        }
    };

    Ok((codeowners_team, fast_display))
}

fn is_match(codeowners_team: Option<&str>, fast_display: &str) -> bool {
    match (codeowners_team, fast_display) {
        (None, "Unowned") => true,
        (Some(t), fd) if fd == t => true,
        _ => false,
    }
}

fn format_mismatch(project: &Project, file_path: &Path, codeowners_display: &str, fast_display: &str) -> String {
    let rel = project.relative_path(file_path).to_string_lossy().to_string();
    format!("- {}: CODEOWNERS={} fast={}", rel, codeowners_display, fast_display)
}
