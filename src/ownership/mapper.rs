use glob_match::glob_match;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

mod directory_mapper;
mod package_mapper;
mod team_file_mapper;
mod team_gem_mapper;
mod team_glob_mapper;
mod team_yml_mapper;

pub use directory_mapper::DirectoryMapper;
pub use package_mapper::JavascriptPackageMapper;
pub use package_mapper::RubyPackageMapper;
pub use team_file_mapper::TeamFileMapper;
pub use team_gem_mapper::TeamGemMapper;
pub use team_glob_mapper::TeamGlobMapper;
pub use team_yml_mapper::TeamYmlMapper;

use super::Entry;

pub trait Mapper {
    fn name(&self) -> String;
    fn entries(&self) -> Vec<Entry>;
    fn owner_matchers(&self) -> Vec<OwnerMatcher>;
}

pub enum OwnerMatcher {
    ExactMatches(HashMap<PathBuf, String>, String),
    Glob { glob: String, team_name: String, source: String },
}

impl OwnerMatcher {
    pub fn owner_for(&self, relative_path: &Path) -> (Option<&String>, &String) {
        match self {
            OwnerMatcher::Glob { glob, team_name, source } => {
                if glob_match(glob, relative_path.to_str().unwrap()) {
                    (Some(team_name), source)
                } else {
                    (None, source)
                }
            }
            OwnerMatcher::ExactMatches(ref path_to_team, source) => (path_to_team.get(relative_path), source),
        }
    }
}
