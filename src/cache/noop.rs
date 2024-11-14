use crate::project::Error;
use error_stack::Result;
use std::path::Path;

use super::{Cache, FileOwnerCacheEntry};

#[derive(Default)]
pub struct NoopCache {}

impl Cache for NoopCache {
    fn get_file_owner(&self, _path: &Path) -> Result<Option<FileOwnerCacheEntry>, Error> {
        Ok(None)
    }

    fn write_file_owner(&self, _path: &Path, _owner: Option<String>) {
        // noop
    }

    fn persist_cache(&self) -> Result<(), Error> {
        Ok(())
    }

    fn delete_cache(&self) -> Result<(), Error> {
        Ok(())
    }
}
