use error_stack::{Result, ResultExt};
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter},
    path::PathBuf,
    sync::Mutex,
};

use crate::project::{Error, ProjectFile};

pub(crate) fn build_project_file(path: PathBuf, use_cache: bool) -> ProjectFile {
    if !use_cache {
        return build_project_file_without_cache(&path);
    }

    if let Ok(Some(cached_project_file)) = get_project_file_from_cache(&path) {
        return cached_project_file;
    }

    let project_file = build_project_file_without_cache(&path);

    if let Ok(mut cache) = PROJECT_FILE_CACHE.lock() {
        cache.insert(path.clone(), project_file.owner.clone().unwrap());
    }
    project_file
}

pub(crate) fn build_project_file_without_cache(path: &PathBuf) -> ProjectFile {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(_) => {
            return ProjectFile {
                path: path.clone(),
                owner: None,
            }
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

lazy_static! {
    static ref TEAM_REGEX: Regex = Regex::new(r#"^(?:#|//) @team (.*)$"#).expect("error compiling regular expression");
    static ref PROJECT_FILE_CACHE: Box<Mutex<HashMap<PathBuf, String>>> =
        Box::new(Mutex::new(load_cache().unwrap_or_else(|_| HashMap::with_capacity(10000))));
}

fn load_cache() -> Result<HashMap<PathBuf, String>, Error> {
    let cache_path = get_cache_path();
    if !cache_path.exists() {
        return Ok(HashMap::with_capacity(10000));
    }

    let file = File::open(cache_path).change_context(Error::Io)?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).change_context(Error::SerdeJson)
}

pub(crate) fn save_cache() -> Result<(), Error> {
    let cache_path = get_cache_path();
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(cache_path)
        .change_context(Error::Io)?;

    let writer = BufWriter::new(file);
    let cache = PROJECT_FILE_CACHE.lock().map_err(|_| Error::Io)?;
    serde_json::to_writer(writer, &*cache).change_context(Error::SerdeJson)
}

fn get_cache_path() -> PathBuf {
    let cache_dir = PathBuf::from("tmp/cache/codeowners");
    fs::create_dir_all(&cache_dir).unwrap();

    cache_dir.join("project_file_cache.json")
}

fn get_project_file_from_cache(path: &PathBuf) -> Result<Option<ProjectFile>, Error> {
    if let Ok(cache) = PROJECT_FILE_CACHE.lock() {
        if let Some(cached_owner) = cache.get(path) {
            return Ok(Some(ProjectFile {
                path: path.clone(),
                owner: Some(cached_owner.clone()),
            }));
        }
    }
    Ok(None)
}
