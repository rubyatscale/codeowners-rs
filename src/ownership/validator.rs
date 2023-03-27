use std::collections::HashMap;
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
pub enum ValidationError {
    FileWithoutOwner { path: PathBuf },
    FileWithMultipleOwners { path: PathBuf, owners: Vec<Owner> },
    CodeownershipFileIsStale,
}

impl Validator {
    #[instrument(level = "debug", skip_all)]
    pub fn validate(&self) -> Vec<ValidationError> {
        let mut validation_errors = Vec::new();

        debug!("validate_file_ownership");
        validation_errors.append(&mut self.validate_file_ownership());

        debug!("validate_codeowners_file");
        validation_errors.append(&mut self.validate_codeowners_file());

        validation_errors
    }

    fn validate_file_ownership(&self) -> Vec<ValidationError> {
        let mut validation_errors = Vec::new();

        for (file, owners) in self.file_to_owners() {
            let relative_path = self.project.relative_path(&file.path).to_owned();

            if owners.is_empty() {
                validation_errors.push(ValidationError::FileWithoutOwner { path: relative_path })
            } else if owners.len() > 1 {
                validation_errors.push(ValidationError::FileWithMultipleOwners {
                    path: relative_path,
                    owners,
                })
            }
        }

        validation_errors
    }

    fn validate_codeowners_file(&self) -> Vec<ValidationError> {
        let generated_file = self.file_generator.generate_file();

        if generated_file != self.project.codeowners_file {
            vec![ValidationError::CodeownershipFileIsStale]
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
