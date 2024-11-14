use crate::project::Error;
use error_stack::Result;
use std::path::Path;

pub mod file;
pub mod noop;

pub trait Cache {
    fn get_file_owner(&self, path: &Path) -> Result<Option<FileOwnerCacheEntry>, Error>;
    fn write_file_owner(&self, path: &Path, owner: Option<String>);
    fn persist_cache(&self) -> Result<(), Error>;
    fn delete_cache(&self) -> Result<(), Error>;
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FileOwnerCacheEntry {
    timestamp: u64,
    pub owner: Option<String>,
}
