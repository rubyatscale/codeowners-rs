use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use error_stack::{Report, Result, ResultExt};
use fast_glob::glob_match;
use ignore::{DirEntry, WalkBuilder, WalkParallel, WalkState};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tracing::{instrument, warn};

use crate::{
    cache::Cache,
    config::Config,
    project::{deserializers, DirectoryCodeownersFile, Error, Package, PackageType, Project, ProjectFile, Team, VendoredGem},
    project_file_builder::ProjectFileBuilder,
    tracked_files,
};

type AbsolutePath = PathBuf;
type RelativePath = PathBuf;

#[derive(Debug)]
enum EntryType {
    Directory(AbsolutePath, RelativePath),
    RubyPackage(AbsolutePath, RelativePath),
    JavascriptPackage(AbsolutePath, RelativePath),
    CodeownerFile(AbsolutePath, RelativePath),
    TeamFile(AbsolutePath, RelativePath),
    OwnedFile(ProjectFile),
    NullEntry(),
}

pub struct ProjectBuilder<'a> {
    config: &'a Config,
    base_path: PathBuf,
    codeowners_file_path: PathBuf,
    project_file_builder: ProjectFileBuilder<'a>,
}

const INITIAL_VECTOR_CAPACITY: usize = 1000;

impl<'a> ProjectBuilder<'a> {
    pub fn new(config: &'a Config, base_path: PathBuf, codeowners_file_path: PathBuf, cache: &'a Cache) -> Self {
        let project_file_builder = ProjectFileBuilder::new(cache);
        Self {
            project_file_builder,
            config,
            base_path,
            codeowners_file_path,
        }
    }

    #[instrument(level = "debug", skip_all, fields(base_path = %self.base_path.display()))]
    pub fn build(&mut self) -> Result<Project, Error> {
        tracing::info!("Starting project build");
        let mut builder = WalkBuilder::new(&self.base_path);
        builder.hidden(false);
        builder.follow_links(false);

        // Prune traversal early: skip heavy and irrelevant directories
        let ignore_dirs = self.config.ignore_dirs.clone();
        let base_path = self.base_path.clone();
        let tracked_files = tracked_files::find_tracked_files(&self.base_path);

        builder.filter_entry(move |entry: &DirEntry| {
            let path = entry.path();
            let file_name = entry.file_name().to_str().unwrap_or("");
            if let Some(tracked_files) = &tracked_files {
                if let Some(ft) = entry.file_type() {
                    if ft.is_file() && !tracked_files.contains_key(path) {
                        return false;
                    }
                }
            }
            if let Some(ft) = entry.file_type() {
                if ft.is_dir() {
                    if let Ok(rel) = path.strip_prefix(&base_path) {
                        if rel.components().count() == 1 && ignore_dirs.iter().any(|d| *d == file_name) {
                            return false;
                        }
                    }
                }
            }

            true
        });

        let walk_parallel: WalkParallel = builder.build_parallel();

        let (tx, rx) = crossbeam_channel::unbounded::<EntryType>();
        let error_holder: Arc<Mutex<Option<Report<Error>>>> = Arc::new(Mutex::new(None));
        let error_holder_for_threads = Arc::clone(&error_holder);

        let this: &ProjectBuilder<'a> = self;

        walk_parallel.run(move || {
            let error_holder = Arc::clone(&error_holder_for_threads);
            let tx = tx.clone();
            Box::new(move |res| {
                if let Ok(entry) = res {
                    match this.build_entry_type(entry) {
                        Ok(entry_type) => {
                            let _ = tx.send(entry_type);
                        }
                        Err(report) => {
                            if let Ok(mut slot) = error_holder.lock() {
                                if slot.is_none() {
                                    *slot = Some(report);
                                }
                            }
                        }
                    }
                }
                WalkState::Continue
            })
        });

        // Take ownership of the collected entry types
        let entry_types: Vec<EntryType> = rx.iter().collect();

        // If any error occurred while building entry types, return it
        let maybe_error = match Arc::try_unwrap(error_holder) {
            Ok(mutex) => match mutex.into_inner() {
                Ok(err_opt) => err_opt,
                Err(poisoned) => poisoned.into_inner(),
            },
            Err(arc) => match arc.lock() {
                Ok(mut guard) => guard.take(),
                Err(poisoned) => poisoned.into_inner().take(),
            },
        };
        if let Some(report) = maybe_error {
            return Err(report);
        }
        self.build_project_from_entry_types(entry_types)
    }

    fn build_entry_type(&self, entry: ignore::DirEntry) -> Result<EntryType, Error> {
        let absolute_path = entry.path();

        let is_dir = entry.file_type().ok_or(Error::Io).change_context(Error::Io)?.is_dir();
        let relative_path = absolute_path.strip_prefix(&self.base_path).change_context(Error::Io)?.to_owned();

        if is_dir {
            return Ok(EntryType::Directory(absolute_path.to_owned(), relative_path.to_owned()));
        }
        let file_name = match relative_path.file_name() {
            Some(name) => name.to_string_lossy().to_lowercase(),
            None => return Ok(EntryType::NullEntry()),
        };

        match file_name.as_str() {
            name if name == "package.yml"
                && relative_path
                    .parent()
                    .is_some_and(|parent| matches_globs(parent, &self.config.ruby_package_paths)) =>
            {
                Ok(EntryType::RubyPackage(absolute_path.to_owned(), relative_path.to_owned()))
            }
            name if name == "package.json"
                && relative_path
                    .parent()
                    .is_some_and(|parent| matches_globs(parent, &self.config.javascript_package_paths)) =>
            {
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
                                if let Some(file_name) = relative_path.file_name() {
                                    gems.push(VendoredGem {
                                        path: absolute_path,
                                        name: file_name.to_string_lossy().to_string(),
                                    });
                                } else {
                                    warn!("Vendored gem path without file name: {:?}", relative_path);
                                }
                            }
                        }
                        EntryType::RubyPackage(absolute_path, relative_path) => {
                            match ruby_package_owner(&absolute_path) {
                                Ok(Some(owner)) => {
                                    pkgs.push(Package {
                                        path: relative_path.clone(),
                                        owner,
                                        package_type: PackageType::Ruby,
                                    });
                                }
                                Ok(None) => { /* No owner, do nothing */ }
                                Err(e) => {
                                    warn!("Error reading ruby package owner for {:?}: {:?}", absolute_path, e);
                                }
                            }
                        }
                        EntryType::JavascriptPackage(absolute_path, relative_path) => {
                            match javascript_package_owner(&absolute_path) {
                                Ok(Some(owner)) => {
                                    pkgs.push(Package {
                                        path: relative_path.clone(),
                                        owner,
                                        package_type: PackageType::Javascript,
                                    });
                                }
                                Ok(None) => { /* No owner, do nothing */ }
                                Err(e) => {
                                    warn!("Error reading javascript package owner for {:?}: {:?}", absolute_path, e);
                                }
                            }
                        }
                        EntryType::CodeownerFile(absolute_path, relative_path) => match std::fs::read_to_string(&absolute_path) {
                            Ok(owner) => {
                                let owner = owner.trim().to_owned();
                                codeowners.push(DirectoryCodeownersFile {
                                    path: relative_path.clone(),
                                    owner,
                                });
                            }
                            Err(e) => {
                                warn!("Error reading codeowner file for {:?}: {:?}", absolute_path, e);
                            }
                        },
                        EntryType::TeamFile(absolute_path, _relative_path) => match Team::from_team_file_path(absolute_path) {
                            Ok(team) => {
                                team_files.push(team);
                            }
                            Err(e) => {
                                warn!("Error building team from team file path: {}", e);
                            }
                        },
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
        let teams_by_name = teams
            .iter()
            .flat_map(|team| vec![(team.name.clone(), team.clone()), (team.github_team.clone(), team.clone())])
            .collect();

        tracing::info!(
            files_count = %project_files.len(),
            teams_count = %teams.len(),
            packages_count = %packages.len(),
            "Project build completed successfully"
        );

        Ok(Project {
            base_path: self.base_path.to_owned(),
            files: project_files,
            vendored_gems,
            teams,
            packages,
            codeowners_file_path: self.codeowners_file_path.to_path_buf(),
            directory_codeowner_files: directory_codeowners,
            teams_by_name,
        })
    }
}

fn matches_globs(path: &Path, globs: &[String]) -> bool {
    match path.to_str() {
        Some(s) => globs.iter().any(|glob| glob_match(glob, s)),
        None => false,
    }
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
        assert!(matches_globs(Path::new("script/.eslintrc.js"), &[OWNED_GLOB.to_string()]));
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match(OWNED_GLOB, "script/.eslintrc.js"));
    }
}
