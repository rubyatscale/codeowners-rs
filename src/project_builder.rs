use std::{
    fs::File,
    path::{Path, PathBuf},
};

use error_stack::{Result, ResultExt};
use fast_glob::glob_match;
use ignore::WalkBuilder;
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tracing::instrument;

use crate::{
    config::Config,
    project::{deserializers, DirectoryCodeownersFile, Error, Package, PackageType, Project, ProjectFile, Team, VendoredGem},
    project_file_builder::ProjectFileBuilder,
};

type AbsolutePath = PathBuf;
type RelativePath = PathBuf;
enum EntryType {
    Directory(AbsolutePath, RelativePath),
    RubyPackage(AbsolutePath, RelativePath),
    JavascriptPackage(AbsolutePath, RelativePath),
    CodeownerFile(AbsolutePath, RelativePath),
    TeamFile(AbsolutePath, RelativePath),
    OwnedFile(ProjectFile),
    NullEntry(),
}

#[derive(Debug)]
pub struct ProjectBuilder<'a> {
    config: &'a Config,
    base_path: PathBuf,
    codeowners_file_path: PathBuf,
    project_file_builder: ProjectFileBuilder<'a>,
}

const INITIAL_VECTOR_CAPACITY: usize = 1000;

impl<'a> ProjectBuilder<'a> {
    pub fn new(config: &'a Config, base_path: PathBuf, codeowners_file_path: PathBuf, use_cache: bool) -> Self {
        let project_file_builder = ProjectFileBuilder::new(config, base_path.clone(), use_cache);
        Self {
            project_file_builder,
            config,
            base_path,
            codeowners_file_path,
        }
    }

    #[instrument(level = "debug", skip_all)]
    pub fn build(&mut self) -> Result<Project, Error> {
        let mut entry_types = Vec::with_capacity(INITIAL_VECTOR_CAPACITY);
        let mut builder = WalkBuilder::new(&self.base_path);
        builder.hidden(false);
        let walkdir = builder.build();

        for entry in walkdir {
            let entry = entry.change_context(Error::Io)?;
            entry_types.push(self.build_entry_type(entry)?);
        }
        let project = self.build_project_from_entry_types(entry_types);
        self.project_file_builder.possibly_save_cache()?;
        project
    }

    fn build_entry_type(&mut self, entry: ignore::DirEntry) -> Result<EntryType, Error> {
        let absolute_path = entry.path();

        let is_dir = entry.file_type().ok_or(Error::Io).change_context(Error::Io)?.is_dir();
        let relative_path = absolute_path.strip_prefix(&self.base_path).change_context(Error::Io)?.to_owned();

        if is_dir {
            return Ok(EntryType::Directory(absolute_path.to_owned(), relative_path.to_owned()));
        }
        let file_name = relative_path
            .file_name()
            .expect("expected a file_name")
            .to_string_lossy()
            .to_lowercase();

        match file_name.as_str() {
            name if name == "package.yml" && matches_globs(relative_path.parent().unwrap(), &self.config.ruby_package_paths) => {
                Ok(EntryType::RubyPackage(absolute_path.to_owned(), relative_path.to_owned()))
            }
            name if name == "package.json" && matches_globs(relative_path.parent().unwrap(), &self.config.javascript_package_paths) => {
                Ok(EntryType::JavascriptPackage(absolute_path.to_owned(), relative_path.to_owned()))
            }
            ".codeowner" => Ok(EntryType::CodeownerFile(absolute_path.to_owned(), relative_path.to_owned())),
            _ if matches_globs(&relative_path, &self.config.team_file_glob) => {
                Ok(EntryType::TeamFile(absolute_path.to_owned(), relative_path.to_owned()))
            }
            _ if matches_globs(&relative_path, &self.config.owned_globs) && !matches_globs(&relative_path, &self.config.unowned_globs) => {
                let project_file = self.project_file_builder.build(absolute_path.to_path_buf());
                Ok(EntryType::OwnedFile(project_file))
            }
            _ => Ok(EntryType::NullEntry()),
        }
    }

    fn build_project_from_entry_types(&mut self, entry_types: Vec<EntryType>) -> Result<Project, Error> {
        let (project_files, packages, vendored_gems, directory_codeowners, teams): (Vec<_>, Vec<_>, Vec<_>, Vec<_>, Vec<_>) = entry_types
            .into_par_iter()
            .fold(
                || {
                    (
                        Vec::<ProjectFile>::with_capacity(INITIAL_VECTOR_CAPACITY),
                        Vec::<Package>::new(),
                        Vec::<VendoredGem>::new(),
                        Vec::<DirectoryCodeownersFile>::new(),
                        Vec::<Team>::new(),
                    )
                },
                |(mut project_files, mut pkgs, mut gems, mut codeowners, mut team_files), entry_type| {
                    match entry_type {
                        EntryType::OwnedFile(project_file) => {
                            project_files.push(project_file);
                        }
                        EntryType::Directory(absolute_path, relative_path) => {
                            if relative_path.parent() == Some(Path::new(&self.config.vendored_gems_path)) {
                                let file_name = relative_path.file_name().expect("expected a file_name");
                                gems.push(VendoredGem {
                                    path: absolute_path,
                                    name: file_name.to_string_lossy().to_string(),
                                });
                            }
                        }
                        EntryType::RubyPackage(absolute_path, relative_path) => {
                            if let Some(owner) = ruby_package_owner(&absolute_path).unwrap() {
                                pkgs.push(Package {
                                    path: relative_path.clone(),
                                    owner,
                                    package_type: PackageType::Ruby,
                                });
                            }
                        }
                        EntryType::JavascriptPackage(absolute_path, relative_path) => {
                            if let Some(owner) = javascript_package_owner(&absolute_path).unwrap() {
                                pkgs.push(Package {
                                    path: relative_path.clone(),
                                    owner,
                                    package_type: PackageType::Javascript,
                                });
                            }
                        }
                        EntryType::CodeownerFile(absolute_path, relative_path) => {
                            let owner = std::fs::read_to_string(absolute_path).unwrap();
                            let owner = owner.trim().to_owned();
                            codeowners.push(DirectoryCodeownersFile {
                                path: relative_path.clone(),
                                owner,
                            });
                        }
                        EntryType::TeamFile(absolute_path, _relative_path) => {
                            let file = File::open(&absolute_path).unwrap();
                            let deserializer: deserializers::Team = serde_yaml::from_reader(file).unwrap();
                            team_files.push(Team {
                                path: absolute_path.to_owned(),
                                name: deserializer.name,
                                github_team: deserializer.github.team,
                                owned_globs: deserializer.owned_globs,
                                owned_gems: deserializer.ruby.map(|ruby| ruby.owned_gems).unwrap_or_default(),
                                avoid_ownership: deserializer.github.do_not_add_to_codeowners_file,
                            });
                        }
                        EntryType::NullEntry() => {}
                    }
                    (project_files, pkgs, gems, codeowners, team_files)
                },
            )
            .reduce(
                || (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new()),
                |mut acc, item| {
                    acc.0.extend(item.0);
                    acc.1.extend(item.1);
                    acc.2.extend(item.2);
                    acc.3.extend(item.3);
                    acc.4.extend(item.4);
                    acc
                },
            );

        Ok(Project {
            base_path: self.base_path.to_owned(),
            files: project_files,
            vendored_gems,
            teams,
            packages,
            codeowners_file_path: self.codeowners_file_path.to_path_buf(),
            directory_codeowner_files: directory_codeowners,
        })
    }
}

fn matches_globs(path: &Path, globs: &[String]) -> bool {
    globs.iter().any(|glob| glob_match(glob, path.to_str().unwrap()))
}

fn ruby_package_owner(path: &Path) -> Result<Option<String>, Error> {
    let file = File::open(path).change_context(Error::Io)?;
    let deserializer: deserializers::RubyPackage = serde_yaml::from_reader(file).change_context(Error::SerdeYaml)?;

    Ok(deserializer.owner)
}

fn javascript_package_owner(path: &Path) -> Result<Option<String>, Error> {
    let file = File::open(path).change_context(Error::Io)?;
    let deserializer: deserializers::JavascriptPackage = serde_json::from_reader(file).change_context(Error::SerdeJson)?;

    Ok(deserializer.metadata.and_then(|metadata| metadata.owner))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OWNED_GLOB: &str = "{app,components,config,frontend,lib,packs,spec,danger,script}/**/*.{rb,arb,erb,rake,js,jsx,ts,tsx}";

    #[test]
    fn test_matches_globs() {
        // should fail because hidden directories are ignored by glob patterns unless explicitly included
        assert!(matches_globs(Path::new("script/.eslintrc.js"), &[OWNED_GLOB.to_string()]));
    }

    #[test]
    fn test_glob_match() {
        // Exposes bug in glob-match https://github.com/devongovett/glob-match/issues/9
        // should fail because hidden directories are ignored by glob patterns unless explicitly included
        assert!(glob_match(OWNED_GLOB, "script/.eslintrc.js"));
    }
}
