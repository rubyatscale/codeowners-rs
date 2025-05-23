use std::sync::Arc;

use super::escaper::escape_brackets;
use super::{Entry, Source};
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
        let team_by_name = self.project.teams_by_name.clone();

        for directory_codeowner_file in &self.project.directory_codeowner_files {
            let dir_root = directory_codeowner_file
                .directory_root()
                .map(|p| p.to_string_lossy())
                .unwrap_or_default();
            let team = team_by_name.get(&directory_codeowner_file.owner);
            if let Some(team) = team {
                entries.push(Entry {
                    path: format!("{}/**/**", escape_brackets(&dir_root)),
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
            owner_matchers.push(OwnerMatcher::new_glob(
                format!(
                    "{}/**/**",
                    escape_brackets(&file.directory_root().map(|p| p.to_string_lossy()).unwrap_or_default())
                ),
                file.owner.to_owned(),
                Source::Directory(file.directory_root().map(|p| p.to_string_lossy().to_string()).unwrap_or_default()),
            ));
        }

        owner_matchers
    }

    fn name(&self) -> String {
        "Owner in .codeowner".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::common_test::tests::{
        build_ownership_with_directory_codeowners, build_ownership_with_directory_codeowners_with_brackets, vecs_match,
    };

    use super::*;
    #[test]
    fn test_entries() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_directory_codeowners()?;
        let mapper = DirectoryMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.entries(),
            &vec![
                Entry {
                    path: "app/consumers/**/**".to_owned(),
                    github_team: "@Bar".to_owned(),
                    team_name: "Bar".to_owned(),
                    disabled: false,
                },
                Entry {
                    path: "app/services/**/**".to_owned(),
                    github_team: "@Foo".to_owned(),
                    team_name: "Foo".to_owned(),
                    disabled: false,
                },
                Entry {
                    path: "app/services/exciting/**/**".to_owned(),
                    github_team: "@Bar".to_owned(),
                    team_name: "Bar".to_owned(),
                    disabled: false,
                },
            ],
        );
        Ok(())
    }

    #[test]
    fn test_entries_with_brackets() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_directory_codeowners_with_brackets()?;
        let mapper = DirectoryMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.entries(),
            &vec![
                Entry {
                    path: "app/\\[consumers\\]/**/**".to_string(),
                    github_team: "@Bar".to_string(),
                    team_name: "Bar".to_string(),
                    disabled: false,
                },
                Entry {
                    path: "app/\\[consumers\\]/deep/nesting/\\[nestdir\\]/**/**".to_string(),
                    github_team: "@Foo".to_string(),
                    team_name: "Foo".to_string(),
                    disabled: false,
                },
            ],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_directory_codeowners()?;
        let mapper = DirectoryMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.owner_matchers(),
            &vec![
                OwnerMatcher::new_glob(
                    "app/consumers/**/**".to_owned(),
                    "Bar".to_owned(),
                    Source::Directory("app/consumers".to_string()),
                ),
                OwnerMatcher::new_glob(
                    "app/services/**/**".to_owned(),
                    "Foo".to_owned(),
                    Source::Directory("app/services".to_owned()),
                ),
                OwnerMatcher::new_glob(
                    "app/services/exciting/**/**".to_owned(),
                    "Bar".to_owned(),
                    Source::Directory("app/services/exciting".to_owned()),
                ),
            ],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers_with_brackets() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_directory_codeowners_with_brackets()?;
        let mapper = DirectoryMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.owner_matchers(),
            &vec![
                OwnerMatcher::new_glob(
                    "app/\\[consumers\\]/**/**".to_string(),
                    "Bar".to_string(),
                    Source::Directory("app/[consumers]".to_string()),
                ),
                OwnerMatcher::new_glob(
                    "app/\\[consumers\\]/deep/nesting/\\[nestdir\\]/**/**".to_string(),
                    "Foo".to_string(),
                    Source::Directory("app/[consumers]/deep/nesting/[nestdir]".to_string()),
                ),
            ],
        );
        Ok(())
    }
}
