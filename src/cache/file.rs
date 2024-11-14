use crate::project::Error;
use error_stack::{Result, ResultExt};
use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter},
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::{Caching, FileOwnerCacheEntry};

#[derive(Debug)]
pub struct GlobalCache {
    base_path: PathBuf,
    cache_directory: String,
    file_owner_cache: Option<Box<Mutex<HashMap<PathBuf, FileOwnerCacheEntry>>>>,
}

const DEFAULT_CACHE_CAPACITY: usize = 10000;

impl Caching for GlobalCache {
    fn get_file_owner(&self, path: &Path) -> Result<Option<FileOwnerCacheEntry>, Error> {
        if let Ok(cache) = self.file_owner_cache.as_ref().unwrap().lock() {
            if let Some(cached_entry) = cache.get(path) {
                let timestamp = get_file_timestamp(path)?;
                if cached_entry.timestamp == timestamp {
                    return Ok(Some(cached_entry.clone()));
                }
            }
        }
        Ok(None)
    }

    fn write_file_owner(&self, path: &Path, owner: Option<String>) {
        if let Ok(mut cache) = self.file_owner_cache.as_ref().unwrap().lock() {
            if let Ok(timestamp) = get_file_timestamp(path) {
                cache.insert(path.to_path_buf(), FileOwnerCacheEntry { timestamp, owner });
            }
        }
    }

    fn persist_cache(&self) -> Result<(), Error> {
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

    fn delete_cache(&self) -> Result<(), Error> {
        let cache_path = self.get_cache_path();
        dbg!("deleting", &cache_path);
        fs::remove_file(cache_path).change_context(Error::Io)
    }
}

impl GlobalCache {
    pub fn new(base_path: PathBuf, cache_directory: String) -> Result<Self, Error> {
        let mut cache = Self {
            base_path,
            cache_directory,
            file_owner_cache: None,
        };
        cache.load_cache().change_context(Error::Io)?;
        Ok(cache)
    }

    fn load_cache(&mut self) -> Result<(), Error> {
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

    fn get_cache_path(&self) -> PathBuf {
        let cache_dir = self.base_path.join(PathBuf::from(&self.cache_directory));
        fs::create_dir_all(&cache_dir).unwrap();

        cache_dir.join("project-file-cache.json")
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_cache_dir() -> Result<(), Error> {
        let temp_dir = tempdir().change_context(Error::Io)?;
        let cache_dir = "test-codeowners-cache";
        let cache = GlobalCache::new(temp_dir.path().to_path_buf(), cache_dir.to_owned())?;

        let file_path = PathBuf::from("tests/fixtures/valid_project/ruby/app/models/bank_account.rb");
        assert!(file_path.exists());
        let timestamp = get_file_timestamp(&file_path)?;

        let cache_entry = cache.get_file_owner(&file_path)?;
        assert_eq!(cache_entry, None);

        cache.write_file_owner(&file_path, Some("owner 1".to_owned()));
        let cache_entry = cache.get_file_owner(&file_path)?;
        assert_eq!(
            cache_entry,
            Some(FileOwnerCacheEntry {
                timestamp,
                owner: Some("owner 1".to_owned())
            })
        );

        cache.persist_cache().change_context(Error::Io)?;
        let persisted_cache_path = cache.get_cache_path();
        assert!(persisted_cache_path.exists());

        let cache = GlobalCache::new(temp_dir.path().to_path_buf(), cache_dir.to_owned())?;
        let cache_entry = cache.get_file_owner(&file_path)?;
        assert_eq!(
            cache_entry,
            Some(FileOwnerCacheEntry {
                timestamp,
                owner: Some("owner 1".to_owned())
            })
        );

        cache.delete_cache().change_context(Error::Io)?;
        assert!(!persisted_cache_path.exists());

        Ok(())
    }

    #[test]
    fn test_corrupted_cache() -> Result<(), Error> {
        let temp_dir = tempdir().change_context(Error::Io)?;
        let cache_dir = "test-codeowners-cache";
        let cache = GlobalCache::new(temp_dir.path().to_path_buf(), cache_dir.to_owned())?;
        let cache_path = cache.get_cache_path();
        fs::write(cache_path, "corrupted_cache").change_context(Error::Io)?;

        // When the cache is corrupted, it should be ignored and a new cache should be created
        let cache = GlobalCache::new(temp_dir.path().to_path_buf(), cache_dir.to_owned())?;
        let file_path = PathBuf::from("tests/fixtures/valid_project/ruby/app/models/bank_account.rb");
        let cache_entry = cache.get_file_owner(&file_path)?;
        assert_eq!(cache_entry, None);
        Ok(())
    }
}
