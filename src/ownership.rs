use std::rc::Rc;
use tracing::{debug, instrument};

mod file_generator;
mod mapper;
mod validator;

#[cfg(test)]
mod tests;

use crate::project::Project;

pub use validator::Errors as ValidatorErrors;

use self::{
    file_generator::FileGenerator,
    mapper::{JavascriptPackageMapper, Mapper, RubyPackageMapper, TeamFileMapper, TeamGemMapper, TeamGlobMapper, TeamYmlMapper},
    validator::Validator,
};

pub struct Ownership {
    project: Rc<Project>,
}

pub struct Entry {
    pub path: String,
    pub github_team: String,
    pub team_name: String,
}

impl Entry {
    fn to_row(&self) -> String {
        format!("/{} {}", self.path, self.github_team)
    }
}

impl Ownership {
    pub fn build(project: Project) -> Self {
        Self { project: Rc::new(project) }
    }

    #[instrument(level = "debug", skip_all)]
    pub fn validate(&self) -> Result<(), ValidatorErrors> {
        debug!("validating file ownership");
        let validator = Validator {
            project: self.project.clone(),
            mappers: self.mappers(),
            file_generator: FileGenerator { mappers: self.mappers() },
        };

        validator.validate()
    }

    #[instrument(level = "debug", skip_all)]
    pub fn generate_file(&self) -> String {
        debug!("generating codeowners file");
        let file_generator = FileGenerator { mappers: self.mappers() };
        file_generator.generate_file()
    }

    fn mappers(&self) -> Vec<Box<dyn Mapper>> {
        vec![
            Box::new(TeamFileMapper::build(self.project.clone())),
            Box::new(TeamGlobMapper::build(self.project.clone())),
            Box::new(RubyPackageMapper::build(self.project.clone())),
            Box::new(JavascriptPackageMapper::build(self.project.clone())),
            Box::new(TeamYmlMapper::build(self.project.clone())),
            Box::new(TeamGemMapper::build(self.project.clone())),
        ]
    }
}
