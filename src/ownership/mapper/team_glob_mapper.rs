use std::sync::Arc;

use super::{Entry, Source};
use super::{Mapper, OwnerMatcher};
use crate::project::Project;

pub struct TeamGlobMapper {
    project: Arc<Project>,
}

impl TeamGlobMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }

    fn iter_team_globs(&self) -> impl Iterator<Item = (&str, &str, &str, bool)> + '_ {
        self.project.teams.iter().flat_map(|team| {
            team.owned_globs
                .iter()
                .map(move |glob| (glob.as_str(), team.github_team.as_str(), team.name.as_str(), team.avoid_ownership))
        })
    }
}

impl Mapper for TeamGlobMapper {
    fn entries(&self) -> Vec<Entry> {
        self.iter_team_globs()
            .map(|(glob, github_team, team_name, disabled)| Entry {
                path: glob.to_owned(),
                github_team: github_team.to_owned(),
                team_name: team_name.to_owned(),
                disabled,
            })
            .collect()
    }

    fn owner_matchers(&self) -> Vec<OwnerMatcher> {
        self.iter_team_globs()
            .map(|(glob, github_team, _, _)| OwnerMatcher::Glob {
                glob: glob.to_owned(),
                team_name: github_team.to_owned(),
                source: Source::TeamGlob(glob.to_owned()),
            })
            .collect()
    }

    fn name(&self) -> String {
        "Team-specific owned globs".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::common_test::tests::{build_ownership_with_all_mappers, build_ownership_with_team_glob_codeowners, vecs_match};

    use super::*;
    #[test]
    fn test_entries() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let mapper = TeamGlobMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.entries(),
            &vec![Entry {
                path: "packs/bar/**".to_owned(),
                github_team: "@Baz".to_owned(),
                team_name: "Baz".to_owned(),
                disabled: false,
            }],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_team_glob_codeowners()?;
        let mapper = TeamGlobMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.owner_matchers(),
            &vec![OwnerMatcher::Glob {
                glob: "packs/bar/**".to_owned(),
                team_name: "@Baz".to_owned(),
                source: Source::TeamGlob("packs/bar/**".to_owned()),
            }],
        );
        Ok(())
    }
}
