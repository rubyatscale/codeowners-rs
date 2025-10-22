use error_stack::Result;
use lazy_static::lazy_static;
use regex::Regex;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::{
    cache::{Cache, Caching},
    project::{Error, ProjectFile},
};

pub struct ProjectFileBuilder<'a> {
    global_cache: &'a Cache,
}

lazy_static! {
    static ref TEAM_REGEX: Regex =
        Regex::new(r#"^(?:#|//|<!--|<%#)\s*(?:@?team:?\s*)(.*?)\s*(?:-->|%>)?$"#).expect("error compiling regular expression");
}

impl<'a> ProjectFileBuilder<'a> {
    pub fn new(global_cache: &'a Cache) -> Self {
        Self { global_cache }
    }

    pub(crate) fn build(&self, path: PathBuf) -> ProjectFile {
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
    let file = match File::open(path) {
        Ok(file) => file,
        Err(_) => {
            return ProjectFile {
                path: path.clone(),
                owner: None,
            };
        }
    };

    let mut reader = BufReader::new(file);
    let mut first_line = String::with_capacity(256);

    match reader.read_line(&mut first_line) {
        Ok(0) | Err(_) => {
            return ProjectFile {
                path: path.clone(),
                owner: None,
            };
        }
        Ok(_) => {}
    }

    // read_line includes the newline, but .lines() doesn't, so we need to trim
    let first_line = first_line.trim_end();

    let owner = TEAM_REGEX
        .captures(first_line)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string());

    ProjectFile { path: path.clone(), owner }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    type FirstLine = &'static str;
    type Owner = &'static str;

    #[test]
    fn test_team_regex() {
        let mut map: HashMap<FirstLine, Owner> = HashMap::new();
        map.insert("// @team Foo", "Foo");
        map.insert("// @team Foo Bar", "Foo Bar");
        map.insert("// @team Zoo", "Zoo");
        map.insert("// @team: Zoo Foo", "Zoo Foo");
        map.insert("# @team: Bap", "Bap");
        map.insert("# @team: Bap Hap", "Bap Hap");
        map.insert("<!-- @team: Zoink -->", "Zoink");
        map.insert("<!-- @team: Zoink Err -->", "Zoink Err");
        map.insert("<%# @team: Zap %>", "Zap");
        map.insert("<%# @team: Zap Zip%>", "Zap Zip");
        map.insert("<!-- @team Blast -->", "Blast");
        map.insert("<!-- @team Blast Off -->", "Blast Off");

        // New team: format (without @ symbol)
        map.insert("# team: MyTeam", "MyTeam");
        map.insert("// team: MyTeam", "MyTeam");
        map.insert("<!-- team: MyTeam -->", "MyTeam");
        map.insert("<%# team: MyTeam %>", "MyTeam");

        for (key, value) in map {
            let owner = TEAM_REGEX.captures(key).and_then(|cap| cap.get(1)).map(|m| m.as_str());
            assert_eq!(owner, Some(value));
        }
    }
}
