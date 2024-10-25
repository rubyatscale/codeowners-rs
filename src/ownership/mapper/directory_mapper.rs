use std::sync::Arc;

use super::Entry;
use super::{Mapper, OwnerMatcher};
use crate::project::Project;

pub struct DirectoryMapper {
    project: Arc<Project>,
}

impl DirectoryMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }
}

impl Mapper for DirectoryMapper {
    fn entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();
        let team_by_name = self.project.team_by_name();

        for directory_codeowner_file in &self.project.directory_codeowner_files {
            let dir_root = directory_codeowner_file.directory_root().to_string_lossy();
            let team = team_by_name.get(&directory_codeowner_file.owner);
            if let Some(team) = team {
                entries.push(Entry {
                    path: format!("{}/**/**", dir_root),
                    github_team: team.github_team.to_owned(),
                    team_name: team.name.to_owned(),
                    disabled: team.avoid_ownership,
                });
            }
        }

        entries
    }

    fn owner_matchers(&self) -> Vec<OwnerMatcher> {
        let mut owner_matchers = Vec::new();

        for file in &self.project.directory_codeowner_files {
            owner_matchers.push(OwnerMatcher::Glob {
                glob: format!("{}/**/**", file.directory_root().to_string_lossy()),
                team_name: file.owner.to_owned(),
                source: format!("directory_mapper ({:?})", &file.directory_root()),
            });
        }

        owner_matchers
    }

    fn name(&self) -> String {
        "Owner in .codeowner".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Package, PackageType, Project, Team};
    use glob_match::glob_match;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[test]
    fn test_multiple_codeowner_files_override() {
        let project = Arc::new(Project {
            packages: vec![],
            teams: vec![
                Team {
                    name: "Foo".to_owned(),
                    github_team: "foo-team".to_owned(),
                    avoid_ownership: false,
                    owned_gems: Vec::new(),
                    owned_globs: Vec::new(),
                    path: PathBuf::from("config/teams/foo.yml"),
                },
                Team {
                    name: "Bar".to_owned(),
                    github_team: "bar-team".to_owned(),
                    avoid_ownership: false,
                    owned_gems: Vec::new(),
                    owned_globs: Vec::new(),
                    path: PathBuf::from("config/teams/bar.yml"),
                },
            ],
            base_path: PathBuf::from("."),
            codeowners_file: String::from(".github/CODEOWNERS"),
            directory_codeowner_files: vec![
                crate::project::DirectoryCodeownersFile {
                    path: PathBuf::from("app/services/.codeowner"),
                    owner: "Foo".to_owned(),
                },
                crate::project::DirectoryCodeownersFile {
                    path: PathBuf::from("app/services/exciting/.codeowner"),
                    owner: "Bar".to_owned(),
                },
            ],
            files: Vec::new(),
            vendored_gems: Vec::new(),
        });

        let mapper = DirectoryMapper::build(project);
        let owner_matchers = mapper.owner_matchers();

        // Check that the more specific .codeowner file takes precedence
        let specific_file = PathBuf::from("app/services/exciting/some_file.rb");
        let matching_owner = owner_matchers.iter().find(|matcher| {
            if let OwnerMatcher::Glob { glob, team_name, .. } = matcher {
                glob_match(glob, specific_file.to_str().unwrap()) && team_name == "Bar"
            } else {
                false
            }
        });
        assert!(matching_owner.is_some(), "Expected to find a matching owner for the specific file");

        // Check that the less specific .codeowner file is also present
        let general_file = PathBuf::from("app/services/other_file.rb");
        let matching_general_owner = owner_matchers.iter().find(|matcher| {
            if let OwnerMatcher::Glob { glob, team_name, .. } = matcher {
                glob_match(glob, general_file.to_str().unwrap()) && team_name == "Foo"
            } else {
                false
            }
        });
        assert!(
            matching_general_owner.is_some(),
            "Expected to find a matching owner for the general file"
        );
    }
}
