use file_owner_finder::FileOwnerFinder;
use mapper::{OwnerMatcher, Source, TeamName};
use std::{
    fmt::{self, Display},
    path::Path,
    sync::Arc,
};
use tracing::{info, instrument};

mod file_generator;
mod file_owner_finder;
pub(crate) mod mapper;
mod validator;

use crate::{ownership::mapper::DirectoryMapper, project::Project};

pub use validator::Errors as ValidatorErrors;

use self::{
    file_generator::FileGenerator,
    mapper::{JavascriptPackageMapper, Mapper, RubyPackageMapper, TeamFileMapper, TeamGemMapper, TeamGlobMapper, TeamYmlMapper},
    validator::Validator,
};

pub struct Ownership {
    project: Arc<Project>,
}

pub struct FileOwner {
    pub team_name: TeamName,
    pub team_config_file_path: String,
    pub sources: Vec<Source>,
}

impl Display for FileOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sources = self
            .sources
            .iter()
            .map(|source| source.to_string())
            .collect::<Vec<String>>()
            .join(", ");
        write!(
            f,
            "Team: {}\nTeam YML: {}\nSource(s): {}",
            self.team_name, self.team_config_file_path, sources
        )
    }
}

impl Default for FileOwner {
    fn default() -> Self {
        Self {
            team_name: "Unowned".to_string(),
            team_config_file_path: "Unowned".to_string(),
            sources: vec![],
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub github_team: String,
    pub team_name: TeamName,
    pub disabled: bool,
}

impl Entry {
    fn to_row(&self) -> String {
        let line = format!("/{} {}", self.path, self.github_team);
        if self.disabled {
            format!("# {}", line)
        } else {
            line
        }
    }
}

impl Ownership {
    pub fn build(project: Project) -> Self {
        Self {
            project: Arc::new(project),
        }
    }

    #[instrument(level = "debug", skip_all)]
    pub fn validate(&self, skip_codeowners_file_validation: bool) -> Result<(), ValidatorErrors> {
        info!("validating file ownership");
        let validator = Validator {
            project: self.project.clone(),
            mappers: self.mappers(),
            file_generator: FileGenerator { mappers: self.mappers() },
        };

        validator.validate(skip_codeowners_file_validation)
    }

    #[instrument(level = "debug", skip_all)]
    pub fn for_file(&self, file_path: &str) -> Result<Vec<FileOwner>, ValidatorErrors> {
        info!("getting file ownership for {}", file_path);
        let owner_matchers: Vec<OwnerMatcher> = self.mappers().iter().flat_map(|mapper| mapper.owner_matchers()).collect();
        let file_owner_finder = FileOwnerFinder {
            owner_matchers: &owner_matchers,
        };
        let owners = file_owner_finder.find(Path::new(file_path));
        Ok(owners
            .iter()
            .map(|owner| match self.project.get_team(&owner.team_name) {
                Some(team) => FileOwner {
                    team_name: owner.team_name.clone(),
                    team_config_file_path: team
                        .path
                        .strip_prefix(&self.project.base_path)
                        .map_or_else(|_| String::new(), |p| p.to_string_lossy().to_string()),
                    sources: owner.sources.clone(),
                },
                None => FileOwner::default(),
            })
            .collect())
    }

    #[instrument(level = "debug", skip_all)]
    pub fn generate_file(&self) -> String {
        info!("generating codeowners file");
        let file_generator = FileGenerator { mappers: self.mappers() };
        file_generator.generate_file()
    }

    fn mappers(&self) -> Vec<Box<dyn Mapper>> {
        vec![
            Box::new(TeamFileMapper::build(self.project.clone())),
            Box::new(TeamGlobMapper::build(self.project.clone())),
            Box::new(DirectoryMapper::build(self.project.clone())),
            Box::new(RubyPackageMapper::build(self.project.clone())),
            Box::new(JavascriptPackageMapper::build(self.project.clone())),
            Box::new(TeamYmlMapper::build(self.project.clone())),
            Box::new(TeamGemMapper::build(self.project.clone())),
        ]
    }
}

#[cfg(test)]
mod tests {
    use crate::common_test::tests::build_ownership_with_all_mappers;

    #[test]
    fn test_for_file_owner() -> Result<(), Box<dyn std::error::Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let file_owners = ownership.for_file("app/consumers/directory_owned.rb").unwrap();
        assert_eq!(file_owners.len(), 1);
        assert_eq!(file_owners[0].team_name, "Bar");
        assert_eq!(file_owners[0].team_config_file_path, "config/teams/bar.yml");
        Ok(())
    }

    #[test]
    fn test_for_file_no_owner() -> Result<(), Box<dyn std::error::Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let file_owners = ownership.for_file("app/madeup/foo.rb").unwrap();
        assert_eq!(file_owners.len(), 0);
        Ok(())
    }
}
