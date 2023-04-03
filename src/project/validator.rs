use core::fmt;
use error_stack::{Report, Result};
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use std::{collections::HashSet, path::PathBuf};
use tracing::debug;

use error_stack::Context;

use super::{Package, ProjectFile, Team};

pub(crate) struct Validator<'a> {
    files: &'a [ProjectFile],
    teams: &'a [Team],
    packages: &'a [Package],
}

impl<'a> Validator<'a> {
    pub fn new(files: &'a [ProjectFile], teams: &'a [Team], packages: &'a [Package]) -> Self {
        Self { files, teams, packages }
    }

    pub fn validate(&self) -> Result<(), Errors> {
        debug!("validating project");
        let mut errors: Vec<Error> = Vec::new();

        let team_names: HashSet<&String> = self.teams.iter().map(|team| &team.name).collect();

        errors.append(&mut self.invalid_team_annotation(&team_names));
        errors.append(&mut self.invalid_package_ownership(&team_names));

        if errors.is_empty() {
            return Ok(());
        }

        let mut report = Report::new(Errors(errors.clone()));

        for error in errors {
            report = report.attach_printable(format!("{}", error));
        }

        Err(report)
    }

    fn invalid_team_annotation(&self, team_names: &HashSet<&String>) -> Vec<Error> {
        self.files
            .par_iter()
            .flat_map(|file| {
                if let Some(owner) = &file.owner {
                    if !team_names.contains(owner) {
                        return Some(Error::InvalidTeam(owner.clone(), file.path.clone()));
                    }
                }

                None
            })
            .collect()
    }

    fn invalid_package_ownership(&self, team_names: &HashSet<&String>) -> Vec<Error> {
        self.packages
            .iter()
            .flat_map(|package| {
                if !team_names.contains(&package.owner) {
                    Some(Error::InvalidTeam(package.owner.clone(), package.path.clone()))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    InvalidTeam(String, PathBuf),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidTeam(team, path) => {
                fmt.write_str(&format!("- {} is referencing an invalid team - '{}'", path.to_string_lossy(), team))
            }
        }
    }
}

impl Context for Error {}

#[derive(Debug, Clone)]
pub struct Errors(Vec<Error>);

impl fmt::Display for Errors {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.write_str("Error")
    }
}

impl Context for Errors {}
