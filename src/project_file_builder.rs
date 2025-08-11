use error_stack::Result;
use lazy_static::lazy_static;
use regex::Regex;
use std::path::{Path, PathBuf};

use crate::{
    cache::{Cache, Caching},
    project::{Error, ProjectFile},
};

pub struct ProjectFileBuilder<'a> {
    global_cache: &'a Cache,
}

lazy_static! {
    static ref TEAM_REGEX: Regex = Regex::new(r#"^(?:#|//) @team (.*)$"#).expect("error compiling regular expression");
}

impl<'a> ProjectFileBuilder<'a> {
    pub fn new(global_cache: &'a Cache) -> Self {
        Self { global_cache }
    }

    pub(crate) fn build(&mut self, path: PathBuf) -> ProjectFile {
        if let Ok(Some(cached_project_file)) = self.get_project_file_from_cache(&path) {
            return cached_project_file;
        }

        let project_file = build_project_file_without_cache(&path);

        self.save_project_file_to_cache(&path, &project_file);

        project_file
    }

    fn get_project_file_from_cache(&self, path: &Path) -> Result<Option<ProjectFile>, Error> {
        self.global_cache.get_file_owner(path).map(|entry| {
            entry.map(|e| ProjectFile {
                path: path.to_path_buf(),
                owner: e.owner,
            })
        })
    }

    fn save_project_file_to_cache(&self, path: &Path, project_file: &ProjectFile) {
        self.global_cache.write_file_owner(path, project_file.owner.clone());
    }
}

pub(crate) fn build_project_file_without_cache(path: &PathBuf) -> ProjectFile {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            return ProjectFile {
                path: path.clone(),
                owner: None,
            };
        }
    };

    let first_line = content.lines().next();
    let Some(first_line) = first_line else {
        return ProjectFile {
            path: path.clone(),
            owner: None,
        };
    };

    let owner = TEAM_REGEX
        .captures(first_line)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string());

    ProjectFile { path: path.clone(), owner }
}
