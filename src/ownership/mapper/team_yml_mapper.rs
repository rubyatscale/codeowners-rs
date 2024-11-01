use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use super::Entry;
use super::{Mapper, OwnerMatcher};
use crate::project::Project;

pub struct TeamYmlMapper {
    project: Arc<Project>,
}

impl TeamYmlMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }
}

impl Mapper for TeamYmlMapper {
    fn entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();

        for team in &self.project.teams {
            entries.push(Entry {
                path: self.project.relative_path(&team.path).to_string_lossy().to_string(),
                github_team: team.github_team.to_owned(),
                team_name: team.name.to_owned(),
                disabled: team.avoid_ownership,
            });
        }

        entries
    }

    fn owner_matchers(&self) -> Vec<OwnerMatcher> {
        let mut path_to_team: HashMap<PathBuf, String> = HashMap::new();

        for team in &self.project.teams {
            path_to_team.insert(self.project.relative_path(&team.path).to_owned(), team.name.to_owned());
        }

        vec![OwnerMatcher::ExactMatches(path_to_team, "team_yml_mapper".to_owned())]
    }

    fn name(&self) -> String {
        "Team YML ownership".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::common_test::tests::{build_ownership_with_all_mappers, build_ownership_with_team_yml_codeowners};

    use super::*;
    #[test]
    fn test_entries() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let mapper = TeamYmlMapper::build(ownership.project.clone());
        let entries = mapper.entries();
        assert_eq!(
            entries,
            vec![
                Entry {
                    path: "config/teams/foo.yml".to_owned(),
                    github_team: "@Foo".to_owned(),
                    team_name: "Foo".to_owned(),
                    disabled: false
                },
                Entry {
                    path: "config/teams/bar.yml".to_owned(),
                    github_team: "@Bar".to_owned(),
                    team_name: "Bar".to_owned(),
                    disabled: false
                },
                Entry {
                    path: "config/teams/bam.yml".to_owned(),
                    github_team: "@Bam".to_owned(),
                    team_name: "Bam".to_owned(),
                    disabled: false
                },
                Entry {
                    path: "config/teams/baz.yml".to_owned(),
                    github_team: "@Baz".to_owned(),
                    team_name: "Baz".to_owned(),
                    disabled: false
                }
            ]
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_team_yml_codeowners()?;
        let mapper = TeamYmlMapper::build(ownership.project.clone());
        let mut owner_matchers = mapper.owner_matchers();
        owner_matchers.sort_by_key(|e| match e {
            OwnerMatcher::Glob { glob, .. } => glob.clone(),
            OwnerMatcher::ExactMatches(_, source) => source.clone(),
        });
        let expected_owner_matchers = vec![OwnerMatcher::ExactMatches(
            HashMap::from([
                (PathBuf::from("config/teams/baz.yml"), "Baz".to_owned()),
                (PathBuf::from("config/teams/bam.yml"), "Bam".to_owned()),
                (PathBuf::from("config/teams/bar.yml"), "Bar".to_owned()),
                (PathBuf::from("config/teams/foo.yml"), "Foo".to_owned()),
            ]),
            "team_yml_mapper".to_owned(),
        )];
        assert_eq!(owner_matchers, expected_owner_matchers);
        Ok(())
    }
}
