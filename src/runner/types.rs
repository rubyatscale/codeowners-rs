use core::fmt;
use std::path::PathBuf;

use error_stack::Context;
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RunResult {
    pub validation_errors: Vec<String>,
    pub io_errors: Vec<String>,
    pub info_messages: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RunConfig {
    pub project_root: PathBuf,
    pub codeowners_file_path: PathBuf,
    pub config_path: PathBuf,
    pub no_cache: bool,
}

#[derive(Debug, Serialize)]
pub enum Error {
    Io(String),
    ValidationFailed,
}

impl Context for Error {}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(msg) => fmt.write_str(msg),
            Error::ValidationFailed => fmt.write_str("Error::ValidationFailed"),
        }
    }
}
