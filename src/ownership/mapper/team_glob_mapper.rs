use std::sync::Arc;

use super::Entry;
use super::{Mapper, OwnerMatcher};
use crate::project::Project;

pub struct TeamGlobMapper {
    project: Arc<Project>,
}

impl TeamGlobMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }
}

impl Mapper for TeamGlobMapper {
    fn entries(&self) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();

        for team in &self.project.teams {
            for owned_glob in &team.owned_globs {
                entries.push(Entry {
                    path: owned_glob.to_owned(),
                    github_team: team.github_team.to_owned(),
                    team_name: team.name.to_owned(),
                    disabled: team.avoid_ownership,
                });
            }
        }

        entries
    }

    fn owner_matchers(&self) -> Vec<OwnerMatcher> {
        let mut owner_matchers: Vec<OwnerMatcher> = Vec::new();

        for team in &self.project.teams {
            for owned_glob in &team.owned_globs {
                owner_matchers.push(OwnerMatcher::Glob {
                    glob: owned_glob.clone(),
                    team_name: team.github_team.clone(),
                    source: "team_glob_mapper".to_owned(),
                })
            }
        }

        owner_matchers
    }

    fn name(&self) -> String {
        "Team-specific owned globs".to_owned()
    }
}
