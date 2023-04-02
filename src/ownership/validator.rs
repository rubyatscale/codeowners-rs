use core::fmt;
use std::collections::HashMap;
use std::fmt::Display;
use std::path::Path;

use std::path::PathBuf;
use std::rc::Rc;

use crate::project::{Project, ProjectFile};

use itertools::Itertools;
use rayon::prelude::IntoParallelRefIterator;
use rayon::prelude::ParallelIterator;
use tracing::debug;
use tracing::instrument;

use super::file_generator::FileGenerator;
use super::mapper::{Mapper, OwnerMatcher};

pub struct Validator {
    pub project: Rc<Project>,
    pub mappers: Vec<Box<dyn Mapper>>,
    pub file_generator: FileGenerator,
}

#[derive(Debug)]
struct Owner {
    pub sources: Vec<String>,
    pub team_name: String,
}

#[derive(Debug)]
enum Error {
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

        if generated_file != self.project.codeowners_file {
            vec![Error::CodeownershipFileIsStale]
        } else {
            vec![]
        }
    }

    fn file_to_owners(&self) -> Vec<(&ProjectFile, Vec<Owner>)> {
        let owner_matchers: Vec<OwnerMatcher> = self.mappers.iter().flat_map(|mapper| mapper.owner_matchers()).collect();

        let files: Vec<(&ProjectFile, &Path)> = self
            .project
            .files
            .iter()
            .filter(|file| !self.project.skip_file(file))
            .map(|file| (file, self.project.relative_path(&file.path)))
            .collect();

        files
            .par_iter()
            .map(|(project_file, relative_path)| {
                let mut owners_and_source: HashMap<&String, Vec<String>> = HashMap::new();

                for owner_matcher in &owner_matchers {
                    let owner = owner_matcher.owner_for(relative_path);

                    if let (Some(owner), source) = owner {
                        let entry = owners_and_source.entry(owner);
                        let sources = entry.or_insert(Vec::new());
                        sources.push(source.to_owned())
                    }
                }

                let owners = owners_and_source
                    .into_iter()
                    .map(|(owner, sources)| Owner {
                        sources,
                        team_name: owner.to_owned(),
                    })
                    .collect_vec();

                (*project_file, owners)
            })
            .collect()
    }
}

impl Error {
    pub fn title(&self) -> String {
        match self {
            Error::FileWithoutOwner { path: _ } => "Some files are missing ownership:".to_owned(),
            Error::FileWithMultipleOwners { path: _, owners: _ } => "Code ownership should only be defined for each file in one way. The following files have declared ownership in multiple ways.".to_owned(),
            Error::CodeownershipFileIsStale => {
                "CODEOWNERS out of date. Run `codeownership generate` to update the CODEOWNERS file".to_owned()
            }
        }
    }

    pub fn messages(&self) -> Vec<String> {
        match self {
            Error::FileWithoutOwner { path } => vec![format!("- {}", path.to_string_lossy())],
            Error::FileWithMultipleOwners { path, owners } => owners
                .iter()
                .flat_map(|owner| {
                    owner
                        .sources
                        .iter()
                        .map(|source| format!("- {} (owner: {}, source: {})", path.to_string_lossy(), owner.team_name, &source))
                        .collect_vec()
                })
                .collect_vec(),
            Error::CodeownershipFileIsStale => vec![],
        }
    }
}

impl Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let grouped_errors = self.0.iter().into_group_map_by(|error| error.title());
        let grouped_errors = Vec::from_iter(grouped_errors.iter());
        let grouped_errors = grouped_errors.iter().sorted_by_key(|(title, _)| title);

        for (title, errors) in grouped_errors {
            write!(f, "\n{}", title)?;

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

impl std::error::Error for Errors {
    fn description(&self) -> &str {
        "Error"
    }
}
