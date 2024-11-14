use crate::project::Error;
use error_stack::{Result, ResultExt};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    sync::Mutex,
};

pub trait Cache {
    fn get_file_owner(&self, path: &Path) -> Result<Option<FileOwnerCacheEntry>, Error>;
    fn write_file_owner(&self, path: &Path, owner: Option<String>);
}

#[derive(Debug)]
pub struct GlobalCache<'a> {
    base_path: &'a PathBuf,
    cache_directory: &'a String,
    file_owner_cache: Option<Box<Mutex<HashMap<PathBuf, FileOwnerCacheEntry>>>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FileOwnerCacheEntry {
    timestamp: u64,
    pub owner: Option<String>,
}

const DEFAULT_CACHE_CAPACITY: usize = 10000;

impl<'a> GlobalCache<'a> {
    pub fn new(base_path: &'a PathBuf, cache_directory: &'a String) -> Self {
        Self {
            base_path,
            cache_directory,
            file_owner_cache: None,
        }
    }

    pub fn persist_cache(&self) -> Result<(), Error> {
        let cache_path = self.get_cache_path();
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(cache_path)
            .change_context(Error::Io)?;

        let writer = BufWriter::new(file);
        let cache = self.file_owner_cache.as_ref().unwrap().lock().map_err(|_| Error::Io)?;
        serde_json::to_writer(writer, &*cache).change_context(Error::SerdeJson)
    }

    pub fn load_cache(&mut self) -> Result<(), Error> {
        let cache_path = self.get_cache_path();
        if !cache_path.exists() {
            self.file_owner_cache = Some(Box::new(Mutex::new(HashMap::with_capacity(DEFAULT_CACHE_CAPACITY))));
            return Ok(());
        }

        let file = File::open(cache_path).change_context(Error::Io)?;
        let reader = BufReader::new(file);
        let json = serde_json::from_reader(reader);
        self.file_owner_cache = match json {
            Ok(cache) => Some(Box::new(Mutex::new(cache))),
            _ => Some(Box::new(Mutex::new(HashMap::with_capacity(DEFAULT_CACHE_CAPACITY)))),
        };
        Ok(())
    }

    pub fn get_file_owner(&self, path: &Path) -> Result<Option<FileOwnerCacheEntry>, Error> {
        if let Ok(cache) = self.file_owner_cache.as_ref().unwrap().lock() {
            if let Some(cached_entry) = cache.get(path) {
                let timestamp = Self::get_file_timestamp(path)?;
                if cached_entry.timestamp == timestamp {
                    return Ok(Some(cached_entry.clone()));
                }
            }
        }
        Ok(None)
    }

    pub fn write_file_owner(&self, path: &Path, owner: Option<String>) {
        if let Ok(mut cache) = self.file_owner_cache.as_ref().unwrap().lock() {
            if let Ok(timestamp) = Self::get_file_timestamp(path) {
                cache.insert(path.to_path_buf(), FileOwnerCacheEntry { timestamp, owner });
            }
        }
    }

    fn get_cache_path(&self) -> PathBuf {
        let cache_dir = self.base_path.join(PathBuf::from(&self.cache_directory));
        fs::create_dir_all(&cache_dir).unwrap();

        cache_dir.join("project-file-cache.json")
    }

    pub fn delete_cache(&self) -> Result<(), Error> {
        let cache_path = self.get_cache_path();
        dbg!("deleting", &cache_path);
        fs::remove_file(cache_path).change_context(Error::Io)
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
}
