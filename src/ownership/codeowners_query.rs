use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ownership::codeowners_file_parser::Parser;
use crate::project::Team;

pub(crate) fn teams_for_files_from_codeowners(
    project_root: &Path,
    codeowners_file_path: &Path,
    team_file_globs: &[String],
    node_limit: crate::yaml::NodeLimit,
    file_paths: &[String],
) -> Result<HashMap<String, Option<Team>>, String> {
    let relative_file_paths: Vec<PathBuf> = file_paths
        .iter()
        .map(Path::new)
        .map(|path| {
            crate::path_utils::project_relative_path(project_root, path)
                .unwrap_or_else(|| crate::path_utils::unplaced_path(project_root, path).to_path_buf())
        })
        .collect();

    let parser = Parser {
        codeowners_file_path: codeowners_file_path.to_path_buf(),
        project_root: project_root.to_path_buf(),
        team_file_globs: team_file_globs.to_vec(),
        node_limit,
    };

    // Keyed by each path as given, so two paths that name the same file are separate entries.
    let teams = parser.teams_from_files_paths(&relative_file_paths).map_err(|e| e.to_string())?;
    Ok(file_paths.iter().cloned().zip(teams).collect())
}
