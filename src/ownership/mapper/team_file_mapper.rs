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
                    if team.avoid_ownership {
                        continue;
                    }

                    let relative_path = self.project.relative_path(&owned_file.path);

                    entries.push(Entry {
                        path: relative_path.to_string_lossy().to_string(),
                        github_team: team.github_team.to_owned(),
                        team_name: team.name.to_owned(),
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
