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

        for team in self.project.teams.iter().filter(|team| !team.avoid_ownership) {
            entries.push(Entry {
                path: self.project.relative_path(&team.path).to_string_lossy().to_string(),
                github_team: team.github_team.to_owned(),
                team_name: team.name.to_owned(),
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
