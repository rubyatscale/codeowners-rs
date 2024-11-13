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

use crate::{
    config::Config,
    project::{Error, ProjectFile},
};

#[derive(Debug)]
pub struct ProjectFileBuilder<'a> {
    config: &'a Config,
    use_cache: bool,
    base_path: PathBuf,
    cache: Option<Box<Mutex<HashMap<PathBuf, CacheEntry>>>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct CacheEntry {
    timestamp: u64,
    owner: Option<String>,
}

lazy_static! {
    static ref TEAM_REGEX: Regex = Regex::new(r#"^(?:#|//) @team (.*)$"#).expect("error compiling regular expression");
}

impl<'a> ProjectFileBuilder<'a> {
    pub fn new(config: &'a Config, base_path: PathBuf, use_cache: bool) -> Self {
        let mut builder = Self {
            config,
            base_path,
            use_cache,
            cache: None,
        };
        if use_cache && builder.load_cache().is_err() {
            dbg!("cache not loaded, creating new cache");
            builder.cache = Some(Box::new(Mutex::new(HashMap::with_capacity(10000))));
        }
        builder
    }

    pub(crate) fn build(&mut self, path: PathBuf) -> ProjectFile {
        if !self.use_cache {
            return build_project_file_without_cache(&path);
        }

        if let Ok(Some(cached_project_file)) = self.get_project_file_from_cache(&path) {
            return cached_project_file;
        }

        let project_file = build_project_file_without_cache(&path);

        self.save_project_file_to_cache(&path, &project_file);

        project_file
    }

    fn load_cache(&mut self) -> Result<(), Error> {
        let cache_path = self.get_cache_path();
        if !cache_path.exists() {
            self.cache = Some(Box::new(Mutex::new(HashMap::with_capacity(10000))));
            return Ok(());
        }

        let file = File::open(cache_path).change_context(Error::Io)?;
        let reader = BufReader::new(file);
        self.cache = Some(Box::new(Mutex::new(
            serde_json::from_reader(reader).change_context(Error::SerdeJson)?,
        )));
        Ok(())
    }

    pub(crate) fn possibly_save_cache(&self) -> Result<(), Error> {
        if !self.use_cache {
            return Ok(());
        }

        let cache_path = self.get_cache_path();
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(cache_path)
            .change_context(Error::Io)?;

        let writer = BufWriter::new(file);
        let cache = self.cache.as_ref().unwrap().lock().map_err(|_| Error::Io)?;
        serde_json::to_writer(writer, &*cache).change_context(Error::SerdeJson)
    }

    fn get_cache_path(&self) -> PathBuf {
        let cache_dir = self.base_path.join(PathBuf::from(&self.config.cache_directory));
        fs::create_dir_all(&cache_dir).unwrap();

        cache_dir.join("project-file-cache.json")
    }

    pub fn delete_cache(&self) -> Result<(), Error> {
        let cache_path = self.get_cache_path();
        fs::remove_file(cache_path).change_context(Error::Io)
    }

    fn get_project_file_from_cache(&self, path: &PathBuf) -> Result<Option<ProjectFile>, Error> {
        if let Ok(cache) = self.cache.as_ref().unwrap().lock() {
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

    fn save_project_file_to_cache(&self, path: &Path, project_file: &ProjectFile) {
        if let Ok(mut cache) = self.cache.as_ref().unwrap().lock() {
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

fn build_project_file_without_cache(path: &PathBuf) -> ProjectFile {
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
