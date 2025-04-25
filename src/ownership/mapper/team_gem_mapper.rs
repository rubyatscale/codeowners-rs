use std::sync::Arc;

use super::{Entry, Source};
use super::{Mapper, OwnerMatcher};
use crate::project::Project;

pub struct TeamGemMapper {
    project: Arc<Project>,
}

impl TeamGemMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }
}

impl Mapper for TeamGemMapper {
    fn entries(&self) -> Vec<Entry> {
        let vendored_gem_by_name = self.project.vendored_gem_by_name();
        let mut entries: Vec<Entry> = Vec::new();

        for team in &self.project.teams {
            for owned_gem in &team.owned_gems {
                let vendored_gem = vendored_gem_by_name.get(owned_gem);

                if let Some(vendored_gem) = vendored_gem {
                    entries.push(Entry {
                        path: format!("{}/**/**", self.project.relative_path(&vendored_gem.path).to_string_lossy()),
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
        let mut owner_matchers: Vec<OwnerMatcher> = Vec::new();
        let vendored_gem_by_name = self.project.vendored_gem_by_name();

        for team in &self.project.teams {
            for owned_gem in &team.owned_gems {
                let vendored_gem = vendored_gem_by_name.get(owned_gem);

                if let Some(vendored_gem) = vendored_gem {
                    owner_matchers.push(OwnerMatcher::new_glob(
                        format!("{}/**/*", self.project.relative_path(&vendored_gem.path).to_string_lossy()),
                        team.name.clone(),
                        Source::TeamGem,
                    ));
                }
            }
        }

        owner_matchers
    }

    fn name(&self) -> String {
        "Team owned gems".to_owned()
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::common_test::tests::{build_ownership_with_all_mappers, build_ownership_with_team_gem_codeowners, vecs_match};

    use super::*;
    #[test]
    fn test_entries() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let mapper = TeamGemMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.entries(),
            &vec![Entry {
                path: "gems/taco/**/**".to_owned(),
                github_team: "@Bam".to_owned(),
                team_name: "Bam".to_owned(),
                disabled: false,
            }],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_team_gem_codeowners()?;
        let mapper = TeamGemMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.owner_matchers(),
            &vec![OwnerMatcher::new_glob(
                "gems/globbing/**/*".to_owned(),
                "Bam".to_owned(),
                Source::TeamGem,
            )],
        );
        Ok(())
    }
}
