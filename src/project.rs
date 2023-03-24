use std::{
    collections::HashMap,
    error::Error,
    fs::File,
    io::BufRead,
    path::{Path, PathBuf},
};

use jwalk::WalkDir;
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use regex::Regex;

use wax::{Glob, Pattern};

use crate::config::CompiledConfig;

pub struct Project {
    pub base_path: PathBuf,
    pub owned_files: Vec<OwnedFile>,
    pub packages: Vec<Package>,
    pub vendored_gems: Vec<VendoredGem>,
    pub teams: Vec<Team>,
}

#[derive(Clone)]
pub struct VendoredGem {
    pub path: PathBuf,
    pub name: String,
}

pub struct OwnedFile {
    pub owner: Option<String>,
    pub path: PathBuf,
}

#[derive(Clone)]
pub struct Team {
    pub path: PathBuf,
    pub name: String,
    pub github_team: String,
    pub owned_globs: Vec<String>,
    pub owned_gems: Vec<String>,
    pub avoid_ownership: bool,
}

pub struct Package {
    pub path: PathBuf,
    pub package_type: PackageType,
    pub owner: String,
}

#[derive(PartialEq, Eq)]
pub enum PackageType {
    Ruby,
    Javascript,
}

mod deserializers {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct Metadata {
        pub owner: Option<String>,
    }

    #[derive(Deserialize)]
    pub struct Package {
        pub metadata: Option<Metadata>,
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
    }

    fn empty_string_vec() -> Vec<String> {
        Vec::new()
    }

    fn bool_false() -> bool {
        false
    }
}

impl Project {
    pub fn build(base_path: &Path, config: &CompiledConfig) -> Result<Self, Box<dyn Error>> {
        let mut owned_file_paths: Vec<PathBuf> = Vec::new();
        let mut packages: Vec<Package> = Vec::new();
        let mut teams: Vec<Team> = Vec::new();
        let mut vendored_gems: Vec<VendoredGem> = Vec::new();

        for entry in WalkDir::new(base_path) {
            let entry = entry?;

            let absolute_path = entry.path();
            let relative_path = absolute_path.strip_prefix(base_path)?.to_owned();

            if entry.file_type().is_dir() {
                if relative_path.parent() == Some(config.vendored_gems_path) {
                    let file_name = relative_path.file_name().expect("expected a file_name");
                    vendored_gems.push(VendoredGem {
                        path: absolute_path,
                        name: file_name.to_string_lossy().to_string(),
                    })
                }

                continue;
            }

            let file_name = relative_path.file_name().expect("expected a file_name");

            if file_name.eq_ignore_ascii_case("package.yml") && matches_globs(&relative_path, &config.ruby_package_paths) {
                if let Some(owner) = package_owner(&absolute_path)? {
                    packages.push(Package {
                        path: relative_path.clone(),
                        owner,
                        package_type: PackageType::Ruby,
                    })
                }
            }

            if file_name.eq_ignore_ascii_case("package.json") && matches_globs(&relative_path, &config.javascript_package_paths) {
                if let Some(owner) = package_owner(&absolute_path)? {
                    packages.push(Package {
                        path: relative_path.clone(),
                        owner,
                        package_type: PackageType::Javascript,
                    })
                }
            }

            if matches_globs(&relative_path, &config.team_file_glob) {
                let deserializer: deserializers::Team = serde_yaml::from_reader(File::open(&absolute_path)?)?;

                teams.push(Team {
                    path: absolute_path.clone(),
                    name: deserializer.name,
                    github_team: deserializer.github.team,
                    owned_globs: deserializer.owned_globs,
                    owned_gems: deserializer.ruby.map(|ruby| ruby.owned_gems).unwrap_or(Vec::new()),
                    avoid_ownership: deserializer.github.do_not_add_to_codeowners_file,
                })
            }

            if matches_globs(&relative_path, &config.owned_file_globs) && !matches_globs(&relative_path, &config.unowned_globs) {
                owned_file_paths.push(absolute_path)
            }
        }

        Ok(Project {
            base_path: base_path.to_owned(),
            owned_files: owned_files(owned_file_paths),
            vendored_gems,
            teams,
            packages,
        })
    }

    pub fn relative_path<'a>(&'a self, absolute_path: &'a Path) -> &'a Path {
        absolute_path
            .strip_prefix(&self.base_path)
            .expect("Could not generate relative path")
    }

    pub fn team_by_name(&self) -> HashMap<String, Team> {
        let mut result: HashMap<String, Team> = HashMap::new();

        for team in &self.teams {
            result.insert(team.name.clone(), team.clone());
        }

        result
    }

    pub fn vendored_gem_by_name(&self) -> HashMap<String, VendoredGem> {
        let mut result: HashMap<String, VendoredGem> = HashMap::new();

        for vendored_gem in &self.vendored_gems {
            result.insert(vendored_gem.name.clone(), vendored_gem.clone());
        }

        result
    }
}

fn owned_files(owned_file_paths: Vec<PathBuf>) -> Vec<OwnedFile> {
    let regexp = Regex::new(r#"^(?:#|//) @team (.*)$"#).expect("error compiling regular expression");

    owned_file_paths
        .into_par_iter()
        .map(|path| {
            let file = File::open(&path).unwrap_or_else(|_| panic!("Couldn't open {}", path.to_string_lossy()));
            let first_line: Result<Option<String>, std::io::Error> = std::io::BufReader::new(file).lines().next().transpose();
            let first_line = first_line.expect("error reading first line");

            if first_line.is_none() {
                return OwnedFile { path, owner: None };
            }

            if let Some(first_line) = first_line {
                let capture = regexp.captures(&first_line);

                if let Some(capture) = capture {
                    let first_capture = capture.get(1);

                    if let Some(first_capture) = first_capture {
                        return OwnedFile {
                            path,
                            owner: Some(first_capture.as_str().to_string()),
                        };
                    }
                }
            }

            OwnedFile { path, owner: None }
        })
        .collect()
}

fn package_owner(path: &Path) -> Result<Option<String>, Box<dyn Error>> {
    let deserializer: deserializers::Package = serde_yaml::from_reader(File::open(path)?)?;

    if let Some(metadata) = deserializer.metadata {
        Ok(metadata.owner)
    } else {
        Ok(None)
    }
}

fn matches_globs(path: &Path, globs: &[Glob]) -> bool {
    globs.iter().any(|glob| glob.is_match(path))
}
