use crate::project::{Project, ProjectFile};
use core::fmt;
use std::collections::HashSet;
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::Arc;

use error_stack::Context;
use itertools::Itertools;
use rayon::prelude::IntoParallelRefIterator;
use rayon::prelude::ParallelIterator;
use tracing::debug;
use tracing::instrument;

use super::file_generator::FileGenerator;
use super::file_owner_finder::FileOwnerFinder;
use super::file_owner_finder::Owner;
use super::mapper::{Mapper, OwnerMatcher, TeamName};

pub struct Validator {
    pub project: Arc<Project>,
    pub mappers: Vec<Box<dyn Mapper>>,
    pub file_generator: FileGenerator,
}

#[derive(Debug)]
enum Error {
    InvalidTeam { name: String, path: PathBuf },
    FileWithoutOwner { path: PathBuf },
    FileWithMultipleOwners { path: PathBuf, owners: Vec<Owner> },
    CodeownershipFileIsStale,
}

#[derive(Debug)]
pub struct Errors(Vec<Error>);

impl Validator {
    #[instrument(level = "debug", skip_all)]
    pub fn validate(&self) -> Result<(), Errors> {
        let mut validation_errors = Vec::new();

        debug!("validate_invalid_team");
        validation_errors.append(&mut self.validate_invalid_team());

        debug!("validate_file_ownership");
        validation_errors.append(&mut self.validate_file_ownership());

        debug!("validate_codeowners_file");
        validation_errors.append(&mut self.validate_codeowners_file());

        if validation_errors.is_empty() {
            Ok(())
        } else {
            Err(Errors(validation_errors))
        }
    }

    fn validate_invalid_team(&self) -> Vec<Error> {
        debug!("validating project");
        let mut errors: Vec<Error> = Vec::new();

        let team_names: HashSet<&TeamName> = self.project.teams.iter().map(|team| &team.name).collect();

        errors.append(&mut self.invalid_team_annotation(&team_names));
        errors.append(&mut self.invalid_package_ownership(&team_names));

        errors
    }

    fn invalid_team_annotation(&self, team_names: &HashSet<&String>) -> Vec<Error> {
        let project = self.project.clone();

        self.project
            .files
            .par_iter()
            .flat_map(|file| {
                if let Some(owner) = &file.owner {
                    if !team_names.contains(owner) {
                        return Some(Error::InvalidTeam {
                            name: owner.clone(),
                            path: project.relative_path(&file.path).to_owned(),
                        });
                    }
                }

                None
            })
            .collect()
    }

    fn invalid_package_ownership(&self, team_names: &HashSet<&String>) -> Vec<Error> {
        self.project
            .packages
            .iter()
            .flat_map(|package| {
                if !team_names.contains(&package.owner) {
                    Some(Error::InvalidTeam {
                        name: package.owner.clone(),
                        path: self.project.relative_path(&package.path).to_owned(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn validate_file_ownership(&self) -> Vec<Error> {
        let mut validation_errors = Vec::new();

        for (file, owners) in self.file_to_owners() {
            let relative_path = self.project.relative_path(&file.path).to_owned();

            if owners.is_empty() {
                validation_errors.push(Error::FileWithoutOwner { path: relative_path })
            } else if owners.len() > 1 {
                validation_errors.push(Error::FileWithMultipleOwners {
                    path: relative_path,
                    owners,
                })
            }
        }

        validation_errors
    }

    fn validate_codeowners_file(&self) -> Vec<Error> {
        let generated_file = self.file_generator.generate_file();

        match self.project.get_codeowners_file() {
            Ok(current_file) => {
                if generated_file != current_file {
                    vec![Error::CodeownershipFileIsStale]
                } else {
                    vec![]
                }
            }
            Err(_) => vec![Error::CodeownershipFileIsStale], // Treat any read error as stale file
        }
    }

    fn file_to_owners(&self) -> Vec<(&ProjectFile, Vec<Owner>)> {
        let owner_matchers: Vec<OwnerMatcher> = self.mappers.iter().flat_map(|mapper| mapper.owner_matchers()).collect();
        let file_owner_finder = FileOwnerFinder {
            owner_matchers: &owner_matchers,
        };
        let project = self.project.clone();

        self.project
            .files
            .par_iter()
            .filter_map(|project_file| {
                let relative_path = project.relative_path(&project_file.path);
                let owners = file_owner_finder.find(relative_path);
                Some((project_file, owners))
            })
            .collect()
    }
}

impl Error {
    pub fn category(&self) -> String {
        match self {
            Error::FileWithoutOwner { path: _ } => "Some files are missing ownership".to_owned(),
            Error::FileWithMultipleOwners { path: _, owners: _ } => "Code ownership should only be defined for each file in one way. The following files have declared ownership in multiple ways".to_owned(),
            Error::CodeownershipFileIsStale => {
                "CODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file".to_owned()
            }
            Error::InvalidTeam { name: _, path: _ } => "Found invalid team annotations".to_owned(),
        }
    }

    pub fn messages(&self) -> Vec<String> {
        match self {
            Error::FileWithoutOwner { path } => vec![format!("- {}", path.to_string_lossy())],
            Error::FileWithMultipleOwners { path, owners } => {
                let path_display = path.to_string_lossy();
                let mut messages = vec![format!("\n{path_display}")];

                owners
                    .iter()
                    .sorted_by_key(|owner| owner.team_name.to_lowercase())
                    .for_each(|owner| {
                        messages.push(format!(" owner: {}", owner.team_name));
                        messages.extend(owner.sources.iter().map(|source| format!("  - {source}")));
                    });

                vec![messages.join("\n")]
            }
            Error::CodeownershipFileIsStale => vec![],
            Error::InvalidTeam { name, path } => vec![format!("- {} is referencing an invalid team - '{}'", path.to_string_lossy(), name)],
        }
    }
}

impl Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let grouped_errors = self.0.iter().into_group_map_by(|error| error.category());
        let grouped_errors = Vec::from_iter(grouped_errors.iter());
        let grouped_errors = grouped_errors.iter().sorted_by_key(|(category, _)| category);

        for (category, errors) in grouped_errors {
            write!(f, "\n{}", category)?;

            let messages = errors.iter().flat_map(|error| error.messages()).sorted().join("\n");
            if !messages.is_empty() {
                writeln!(f)?;
                write!(f, "{}", &messages)?;
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

impl Context for Errors {}
