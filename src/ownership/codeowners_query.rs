use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ownership::codeowners_file_parser::Parser;
use crate::project::Team;

pub(crate) fn team_for_file_from_codeowners(
    project_root: &Path,
    codeowners_file_path: &Path,
    team_file_globs: &[String],
    file_path: &Path,
) -> Result<Option<Team>, String> {
    let relative_file_path = if file_path.is_absolute() {
        crate::path_utils::relative_to_buf(project_root, file_path)
    } else {
        PathBuf::from(file_path)
    };

    let parser = Parser {
        codeowners_file_path: codeowners_file_path.to_path_buf(),
        project_root: project_root.to_path_buf(),
        team_file_globs: team_file_globs.to_vec(),
    };

    parser.team_from_file_path(&relative_file_path).map_err(|e| e.to_string())
}

pub(crate) fn teams_for_files_from_codeowners(
    project_root: &Path,
    codeowners_file_path: &Path,
    team_file_globs: &[String],
    file_paths: &[String],
) -> Result<HashMap<String, Option<Team>>, String> {
    let relative_file_paths: Vec<PathBuf> = file_paths
        .iter()
        .map(Path::new)
        .map(|path| {
            if path.is_absolute() {
                crate::path_utils::relative_to_buf(project_root, path)
            } else {
                path.to_path_buf()
            }
        })
        .collect();

    let parser = Parser {
        codeowners_file_path: codeowners_file_path.to_path_buf(),
        project_root: project_root.to_path_buf(),
        team_file_globs: team_file_globs.to_vec(),
    };

    parser.teams_from_files_paths(&relative_file_paths).map_err(|e| e.to_string())
}
