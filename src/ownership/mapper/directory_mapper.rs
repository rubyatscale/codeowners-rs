use std::collections::HashMap;
use std::path::PathBuf;
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
