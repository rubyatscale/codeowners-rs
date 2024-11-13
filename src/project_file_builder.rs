use error_stack::{Result, ResultExt};
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
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

    save_project_file_to_cache(&path, &project_file);

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

#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    timestamp: u64,
    owner: Option<String>,
}

lazy_static! {
    static ref TEAM_REGEX: Regex = Regex::new(r#"^(?:#|//) @team (.*)$"#).expect("error compiling regular expression");
    static ref PROJECT_FILE_CACHE: Box<Mutex<HashMap<PathBuf, CacheEntry>>> =
        Box::new(Mutex::new(load_cache().unwrap_or_else(|_| HashMap::with_capacity(10000))));
}

fn load_cache() -> Result<HashMap<PathBuf, CacheEntry>, Error> {
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
        if let Some(cached_entry) = cache.get(path) {
            let timestamp = get_file_timestamp(path)?;
            if cached_entry.timestamp == timestamp {
                return Ok(Some(ProjectFile {
                    path: path.clone(),
                    owner: cached_entry.owner.clone(),
                }));
            }
        }
    }
    Ok(None)
}

fn save_project_file_to_cache(path: &Path, project_file: &ProjectFile) {
    if let Ok(mut cache) = PROJECT_FILE_CACHE.lock() {
        if let Ok(timestamp) = get_file_timestamp(path) {
            cache.insert(
                path.to_path_buf(),
                CacheEntry {
                    timestamp,
                    owner: project_file.owner.clone(),
                },
            );
        }
    }
}

fn get_file_timestamp(path: &Path) -> Result<u64, Error> {
    let metadata = fs::metadata(path).change_context(Error::Io)?;
    metadata
        .modified()
        .change_context(Error::Io)?
        .duration_since(std::time::UNIX_EPOCH)
        .change_context(Error::Io)
        .map(|duration| duration.as_secs())
}
