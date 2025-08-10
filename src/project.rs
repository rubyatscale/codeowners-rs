use core::fmt;
use std::{
    collections::HashMap,
    fmt::Display,
    fs::File,
    path::{Path, PathBuf},
};

use error_stack::{Context, Result, ResultExt};

pub struct Project {
    pub base_path: PathBuf,
    pub files: Vec<ProjectFile>,
    pub packages: Vec<Package>,
    pub vendored_gems: Vec<VendoredGem>,
    pub teams: Vec<Team>,
    pub codeowners_file_path: PathBuf,
    pub directory_codeowner_files: Vec<DirectoryCodeownersFile>,
    pub teams_by_name: HashMap<String, Team>,
}

#[derive(Clone, Debug)]
pub struct VendoredGem {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ProjectFile {
    pub owner: Option<String>,
    pub path: PathBuf,
}

#[derive(Clone, Debug, Default)]
pub struct Team {
    pub path: PathBuf,
    pub name: String,
    pub github_team: String,
    pub owned_globs: Vec<String>,
    pub subtracted_globs: Vec<String>,
    pub owned_gems: Vec<String>,
    pub avoid_ownership: bool,
}

impl Team {
    pub fn from_team_file_path(absolute_path: PathBuf) -> Result<Self, Error> {
        let file = File::open(&absolute_path).change_context(Error::Io)?;
        let deserializer: deserializers::Team = serde_yaml::from_reader(file).change_context(Error::SerdeYaml)?;
        Ok(Self {
            path: absolute_path.to_owned(),
            name: deserializer.name,
            github_team: deserializer.github.team,
            owned_globs: deserializer.owned_globs,
            subtracted_globs: deserializer.subtracted_globs,
            owned_gems: deserializer.ruby.map(|ruby| ruby.owned_gems).unwrap_or_default(),
            avoid_ownership: deserializer.github.do_not_add_to_codeowners_file,
        })
    }
}

#[derive(Clone, Debug)]
pub struct Package {
    pub path: PathBuf,
    pub package_type: PackageType,
    pub owner: String,
}

impl Package {
    pub fn package_root(&self) -> Option<&Path> {
        self.path.parent()
    }
}

#[derive(Clone, Debug)]
pub struct DirectoryCodeownersFile {
    pub path: PathBuf,
    pub owner: String,
}

impl DirectoryCodeownersFile {
    pub fn directory_root(&self) -> Option<&Path> {
        self.path.parent()
    }
}

#[derive(PartialEq, Eq, Debug, Clone)]
pub enum PackageType {
    Ruby,
    Javascript,
}

impl Display for PackageType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

pub mod deserializers {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct Metadata {
        pub owner: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct JavascriptPackage {
        pub metadata: Option<Metadata>,
    }

    #[derive(Deserialize)]
    pub struct RubyPackage {
        pub owner: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct Github {
        pub team: String,
        #[serde(default = "bool_false")]
        pub do_not_add_to_codeowners_file: bool,
    }

    #[derive(Deserialize)]
    pub struct Ruby {
        #[serde(default = "empty_string_vec")]
        pub owned_gems: Vec<String>,
    }

    #[derive(Deserialize)]
    pub struct Team {
        pub name: String,
        pub github: Github,
        pub ruby: Option<Ruby>,

        #[serde(default = "empty_string_vec")]
        pub owned_globs: Vec<String>,

        #[serde(alias = "unowned_globs", default = "empty_string_vec")]
        pub subtracted_globs: Vec<String>,
    }

    fn empty_string_vec() -> Vec<String> {
        Vec::new()
    }

    fn bool_false() -> bool {
        false
    }
}

#[derive(Debug)]
pub enum Error {
    Io,
    SerdeYaml,
    SerdeJson,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io => fmt.write_str("Error::Io"),
            Error::SerdeYaml => fmt.write_str("Error::SerdeYaml"),
            Error::SerdeJson => fmt.write_str("Error::SerdeJson"),
        }
    }
}

impl Context for Error {}

impl Project {
    pub fn get_codeowners_file(&self) -> Result<String, Error> {
        let codeowners_file: String = if self.codeowners_file_path.exists() {
            std::fs::read_to_string(&self.codeowners_file_path).change_context(Error::Io)?
        } else {
            "".to_owned()
        };
        Ok(codeowners_file)
    }

    pub fn relative_path<'a>(&'a self, absolute_path: &'a Path) -> &'a Path {
        absolute_path.strip_prefix(&self.base_path).unwrap_or(absolute_path)
    }

    pub fn get_team(&self, name: &str) -> Option<Team> {
        self.teams_by_name.get(name).cloned()
    }

    pub fn vendored_gem_by_name(&self) -> HashMap<String, VendoredGem> {
        let mut result: HashMap<String, VendoredGem> = HashMap::new();

        for vendored_gem in &self.vendored_gems {
            result.insert(vendored_gem.name.clone(), vendored_gem.clone());
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vendored_gem_by_name_maps_all_gems() {
        let vg1 = VendoredGem { path: PathBuf::from("vendored/a"), name: "a".to_string() };
        let vg2 = VendoredGem { path: PathBuf::from("vendored/b"), name: "b".to_string() };
        let project = Project {
            base_path: PathBuf::from("."),
            files: vec![],
            packages: vec![],
            vendored_gems: vec![vg1.clone(), vg2.clone()],
            teams: vec![],
            codeowners_file_path: PathBuf::from(".github/CODEOWNERS"),
            directory_codeowner_files: vec![],
            teams_by_name: HashMap::new(),
        };

        let map = project.vendored_gem_by_name();
        assert_eq!(map.len(), 2);
        assert_eq!(map.get("a").unwrap().name, vg1.name);
        assert_eq!(map.get("b").unwrap().name, vg2.name);
    }
}
