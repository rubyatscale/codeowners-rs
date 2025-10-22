use std::path::Path;

use crate::{
    config::Config,
    ownership::file_owner_resolver::find_file_owners,
    project::Project,
    project_builder::ProjectBuilder,
    runner::{RunConfig, RunResult, config_from_path, team_for_file_from_codeowners},
};

pub fn crosscheck_owners(run_config: &RunConfig) -> RunResult {
    match do_crosscheck_owners(run_config) {
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

fn do_crosscheck_owners(run_config: &RunConfig) -> Result<Vec<String>, String> {
    let config = load_config(run_config)?;
    let project = build_project(&config, run_config)?;

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

fn build_project(config: &Config, run_config: &RunConfig) -> Result<Project, String> {
    let mut project_builder = ProjectBuilder::new(config, run_config.project_root.clone(), run_config.codeowners_file_path.clone());
    project_builder.build().map_err(|e| e.to_string())
}

fn owners_for_file(path: &Path, run_config: &RunConfig, config: &Config) -> Result<(Option<String>, String), String> {
    let file_path_str = path.to_string_lossy().to_string();

    let codeowners_team = team_for_file_from_codeowners(run_config, &file_path_str)
        .map_err(|e| e.to_string())?
        .map(|t| t.name);

    let fast_owners = find_file_owners(&run_config.project_root, config, Path::new(&file_path_str))?;
    let fast_display = match fast_owners.len() {
        0 => "Unowned".to_string(),
        1 => fast_owners[0].team.name.clone(),
        _ => {
            let names: Vec<String> = fast_owners.into_iter().map(|fo| fo.team.name).collect();
            format!("Multiple: {}", names.join(", "))
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
