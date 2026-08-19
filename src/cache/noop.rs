use crate::project::Error;
use error_stack::Report;
use std::path::Path;

use super::{Caching, FileOwnerCacheEntry};

#[derive(Default)]
pub struct NoopCache {}

impl Caching for NoopCache {
    fn get_file_owner(&self, _path: &Path) -> Result<Option<FileOwnerCacheEntry>, Report<Error>> {
        Ok(None)
    }

    fn write_file_owner(&self, _path: &Path, _owner: Option<String>) {
        // noop
    }

    fn persist_cache(&self) -> Result<(), Report<Error>> {
        Ok(())
    }

    fn delete_cache(&self) -> Result<(), Report<Error>> {
        Ok(())
    }
}
