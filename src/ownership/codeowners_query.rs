use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::ownership::codeowners_file_parser::Parser;
use crate::project::Team;

pub(crate) fn teams_for_files_from_codeowners(
    project_root: &Path,
    codeowners_file_path: &Path,
    team_file_globs: &[String],
    file_paths: &[String],
) -> Result<HashMap<String, Option<Team>>, String> {
    // Normalize the same way `Runner::validate_files` does. This is reached from public API
    // (`runner::teams_for_files_from_codeowners`) and had the same defect:
    // `relative_to_buf` passes an unstrippable path through unchanged, so an absolute path
    // that disagreed with `project_root` about symlinks -- a `/var/...` path against a
    // `/private/var/...` root -- was looked up in the CODEOWNERS file *as an absolute path*,
    // matched no entry, and came back unowned.
    //
    // Falls back to the path as given rather than dropping it, because the returned map is
    // contracted to hold one entry per input and `team_for_file_from_codeowners` asserts on
    // that. A path that cannot be placed inside the project has no owner, which is the
    // honest answer for a lookup.
    let canonical_root = project_root.canonicalize().ok();
    let relative_file_paths: Vec<PathBuf> = file_paths
        .iter()
        .map(Path::new)
        .map(|path| {
            crate::path_utils::resolve_project_relative(project_root, canonical_root.as_deref(), path).unwrap_or_else(|| path.to_path_buf())
        })
        .collect();

    let parser = Parser {
        codeowners_file_path: codeowners_file_path.to_path_buf(),
        project_root: project_root.to_path_buf(),
        team_file_globs: team_file_globs.to_vec(),
    };

    parser.teams_from_files_paths(&relative_file_paths).map_err(|e| e.to_string())
}
