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
            let team_subtracted_globs = team.subtracted_globs.clone();
            for owned_glob in &team.owned_globs {
                owner_matchers.push(OwnerMatcher::new_glob_with_candidate_subtracted_globs(
                    owned_glob.clone(),
                    &team_subtracted_globs,
                    team.github_team.clone(),
                    Source::TeamGlob(owned_glob.clone()),
                ))
            }
        }

        owner_matchers
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::common_test::tests::{
        build_ownership_with_all_mappers, build_ownership_with_subtracted_globs_team_glob_codeowners,
        build_ownership_with_team_glob_codeowners, vecs_match,
    };

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
            &vec![OwnerMatcher::new_glob_with_candidate_subtracted_globs(
                "packs/bar/**".to_owned(),
                &[],
                "@Baz".to_owned(),
                Source::TeamGlob("packs/bar/**".to_owned()),
            )],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers_with_subtracted_globs() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_subtracted_globs_team_glob_codeowners()?;

        let mapper = TeamGlobMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.owner_matchers(),
            &vec![OwnerMatcher::new_glob_with_candidate_subtracted_globs(
                "packs/bar/**".to_owned(),
                &["packs/bar/excluded/**".to_owned()],
                "@Baz".to_owned(),
                Source::TeamGlob("packs/bar/**".to_owned()),
            )],
        );
        Ok(())
    }
}
