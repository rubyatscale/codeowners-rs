use std::{collections::HashMap, path::Path};

use super::mapper::{OwnerMatcher, Source, TeamName};

#[derive(Debug)]
pub struct Owner {
    pub sources: Vec<Source>,
    pub team_name: TeamName,
}

pub struct FileOwnerFinder<'a> {
    pub owner_matchers: &'a [OwnerMatcher],
}

impl FileOwnerFinder<'_> {
    pub fn find(&self, relative_path: &Path) -> Vec<Owner> {
        let mut team_sources_map: HashMap<&TeamName, Vec<Source>> = HashMap::new();
        let mut directory_overrider = DirectoryOverrider::default();

        for owner_matcher in self.owner_matchers {
            let (owner, source) = owner_matcher.owner_for(relative_path);

            if let Some(team_name) = owner {
                match source {
                    Source::Directory(_) => {
                        directory_overrider.process(team_name, source);
                    }
                    _ => {
                        team_sources_map.entry(team_name).or_default().push(source.clone());
                    }
                }
            }
        }

        // Add most specific directory owner if it exists
        if let Some((team_name, source)) = directory_overrider.specific_directory_owner() {
            team_sources_map.entry(team_name).or_default().push(source.clone());
        }

        team_sources_map
            .into_iter()
            .map(|(team_name, sources)| Owner {
                sources,
                team_name: team_name.clone(),
            })
            .collect()
    }
}

/// DirectoryOverrider is used to override the owner of a directory if a more specific directory owner is found.
#[derive(Debug, Default)]
pub struct DirectoryOverrider<'a> {
    specific_directory_owner: Option<(&'a TeamName, &'a Source)>,
}

impl<'a> DirectoryOverrider<'a> {
    fn process(&mut self, team_name: &'a TeamName, source: &'a Source) {
        if self
            .specific_directory_owner
            .is_none_or(|(_, current_source)| current_source.len() < source.len())
        {
            self.specific_directory_owner = Some((team_name, source));
        }
    }

    fn specific_directory_owner(&self) -> Option<(&TeamName, &Source)> {
        self.specific_directory_owner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directory_overrider() {
        let mut directory_overrider = DirectoryOverrider::default();
        assert_eq!(directory_overrider.specific_directory_owner(), None);
        let team_name_1 = "team1".to_string();
        let source_1 = Source::Directory("src/**".to_string());
        directory_overrider.process(&team_name_1, &source_1);
        assert_eq!(directory_overrider.specific_directory_owner(), Some((&team_name_1, &source_1)));

        let team_name_longest = "team2".to_string();
        let source_longest = Source::Directory("source/subdir/**".to_string());
        directory_overrider.process(&team_name_longest, &source_longest);
        assert_eq!(
            directory_overrider.specific_directory_owner(),
            Some((&team_name_longest, &source_longest))
        );

        let team_name_3 = "team3".to_string();
        let source_3 = Source::Directory("source/**".to_string());
        directory_overrider.process(&team_name_3, &source_3);
        assert_eq!(
            directory_overrider.specific_directory_owner(),
            Some((&team_name_longest, &source_longest))
        );
    }
}
