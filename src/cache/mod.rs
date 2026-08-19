use crate::project::Error;
use enum_dispatch::enum_dispatch;
use error_stack::Report;
use file::GlobalCache;
use noop::NoopCache;
use std::path::Path;

pub mod file;
pub mod noop;

#[enum_dispatch]
pub enum Cache {
    GlobalCache,
    NoopCache,
}

#[enum_dispatch(Cache)]
pub trait Caching {
    fn get_file_owner(&self, path: &Path) -> Result<Option<FileOwnerCacheEntry>, Report<Error>>;
    fn write_file_owner(&self, path: &Path, owner: Option<String>);
    fn persist_cache(&self) -> Result<(), Report<Error>>;
    fn delete_cache(&self) -> Result<(), Report<Error>>;
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct FileOwnerCacheEntry {
    timestamp: u64,
    pub owner: Option<String>,
}
