use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::Entry;
use super::{Mapper, OwnerMatcher};
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
                        path: relative_path.to_string_lossy().to_string(),
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

        vec![OwnerMatcher::ExactMatches(path_to_team, "team_file_mapper".to_owned())]
    }

    fn name(&self) -> String {
        "Annotations at the top of file".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::common_test::tests::{build_ownership_with_all_mappers, build_ownership_with_team_file_codeowners};

    use super::*;
    #[test]
    fn test_entries() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let mapper = TeamFileMapper::build(ownership.project.clone());
        let mut entries = mapper.entries();
        entries.sort_by_key(|e| e.path.clone());
        assert_eq!(
            entries,
            vec![
                Entry {
                    path: "packs/jscomponents/comp.ts".to_owned(),
                    github_team: "@Foo".to_owned(),
                    team_name: "Foo".to_owned(),
                    disabled: false
                },
                Entry {
                    path: "packs/zebra/app/services/team_file_owned.rb".to_owned(),
                    github_team: "@Foo".to_owned(),
                    team_name: "Foo".to_owned(),
                    disabled: false
                }
            ]
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_team_file_codeowners()?;
        let mapper = TeamFileMapper::build(ownership.project.clone());
        let owner_matchers = mapper.owner_matchers();
        let expected_owner_matchers = vec![OwnerMatcher::ExactMatches(HashMap::new(), "team_file_mapper".to_owned())];
        assert_eq!(owner_matchers, expected_owner_matchers);
        Ok(())
    }
}
