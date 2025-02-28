use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::Entry;
use super::escaper::escape_brackets;
use super::{Mapper, OwnerMatcher};
use crate::ownership::mapper::Source;
use crate::project::Project;

pub struct TeamFileMapper {
    project: Arc<Project>,
}

impl TeamFileMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }
}

impl Mapper for TeamFileMapper {
    fn entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();
        let team_by_name = self.project.team_by_name();

        for owned_file in &self.project.files {
            if let Some(ref owner) = owned_file.owner {
                let team = team_by_name.get(owner);

                if let Some(team) = team {
                    let relative_path = self.project.relative_path(&owned_file.path);

                    entries.push(Entry {
                        path: escape_brackets(&relative_path.to_string_lossy()),
                        github_team: team.github_team.to_owned(),
                        team_name: team.name.to_owned(),
                        disabled: team.avoid_ownership,
                    });
                }
            }
        }

        entries
    }

    fn owner_matchers(&self) -> Vec<OwnerMatcher> {
        let team_by_name = self.project.team_by_name();

        let mut path_to_team: HashMap<PathBuf, String> = HashMap::new();

        for owned_file in &self.project.files {
            if let Some(ref owner) = owned_file.owner {
                let team = team_by_name.get(owner);

                if let Some(team) = team {
                    let relative_path = self.project.relative_path(&owned_file.path);
                    path_to_team.insert(relative_path.to_owned(), team.name.clone());
                }
            }
        }

        vec![OwnerMatcher::ExactMatches(path_to_team, Source::TeamFile)]
    }

    fn name(&self) -> String {
        "Annotations at the top of file".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::common_test::tests::{build_ownership_with_team_file_codeowners, vecs_match};

    use super::*;
    #[test]
    fn test_entries() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_team_file_codeowners()?;
        let mapper = TeamFileMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.entries(),
            &vec![
                Entry {
                    path: "packs/\\[admin\\]/comp.ts".to_owned(),
                    github_team: "@Bar".to_owned(),
                    team_name: "Bar".to_owned(),
                    disabled: false,
                },
                Entry {
                    path: "packs/jscomponents/comp.ts".to_owned(),
                    github_team: "@Foo".to_owned(),
                    team_name: "Foo".to_owned(),
                    disabled: false,
                },
                Entry {
                    path: "packs/bar/comp.rb".to_owned(),
                    github_team: "@Bar".to_owned(),
                    team_name: "Bar".to_owned(),
                    disabled: false,
                },
            ],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_team_file_codeowners()?;
        let mapper = TeamFileMapper::build(ownership.project.clone());
        let owner_matchers = mapper.owner_matchers();
        let expected_owner_matchers = vec![OwnerMatcher::ExactMatches(
            HashMap::from([
                (PathBuf::from("packs/[admin]/comp.ts"), "Bar".to_owned()),
                (PathBuf::from("packs/bar/comp.rb"), "Bar".to_owned()),
                (PathBuf::from("packs/jscomponents/comp.ts"), "Foo".to_owned()),
            ]),
            Source::TeamFile,
        )];
        assert_eq!(owner_matchers, expected_owner_matchers);
        Ok(())
    }
}
