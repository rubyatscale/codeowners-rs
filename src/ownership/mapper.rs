use directory_mapper::is_directory_mapper_source;
use glob_match::glob_match;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

mod directory_mapper;
mod package_mapper;
mod team_file_mapper;
mod team_gem_mapper;
mod team_glob_mapper;
mod team_yml_mapper;

pub use directory_mapper::DirectoryMapper;
pub use package_mapper::JavascriptPackageMapper;
pub use package_mapper::RubyPackageMapper;
pub use team_file_mapper::TeamFileMapper;
pub use team_gem_mapper::TeamGemMapper;
pub use team_glob_mapper::TeamGlobMapper;
pub use team_yml_mapper::TeamYmlMapper;

use super::Entry;

pub trait Mapper {
    fn name(&self) -> String;
    fn entries(&self) -> Vec<Entry>;
    fn owner_matchers(&self) -> Vec<OwnerMatcher>;
}
pub type TeamName = String;
pub type Source = String;

#[derive(Debug, PartialEq)]
pub enum OwnerMatcher {
    ExactMatches(HashMap<PathBuf, TeamName>, Source),
    Glob { glob: String, team_name: TeamName, source: Source },
}

impl OwnerMatcher {
    pub fn owner_for(&self, relative_path: &Path) -> (Option<&TeamName>, &Source) {
        match self {
            OwnerMatcher::Glob { glob, team_name, source } => {
                if glob_match(glob, relative_path.to_str().unwrap()) {
                    (Some(team_name), source)
                } else {
                    (None, source)
                }
            }
            OwnerMatcher::ExactMatches(ref path_to_team, source) => (path_to_team.get(relative_path), source),
        }
    }
}

#[derive(Debug)]
pub struct Owner {
    pub sources: Vec<Source>,
    pub team_name: TeamName,
}

pub struct FileOwnerFinder<'a> {
    pub owner_matchers: &'a [OwnerMatcher],
}

impl<'a> FileOwnerFinder<'a> {
    pub fn find(&self, relative_path: &Path) -> Vec<Owner> {
        let mut team_sources_map: HashMap<&TeamName, Vec<Source>> = HashMap::new();
        let mut directory_overrider = DirectoryOverrider::default();

        for owner_matcher in self.owner_matchers {
            let (owner, source) = owner_matcher.owner_for(relative_path);

            if let Some(team_name) = owner {
                if is_directory_mapper_source(source) {
                    directory_overrider.process(team_name, source);
                } else {
                    team_sources_map.entry(team_name).or_default().push(source.clone());
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
struct DirectoryOverrider<'a> {
    specific_directory_owner: Option<(&'a TeamName, &'a Source)>,
}

impl<'a> DirectoryOverrider<'a> {
    fn process(&mut self, team_name: &'a TeamName, source: &'a Source) {
        if self
            .specific_directory_owner
            .map_or(true, |(_, current_source)| current_source.len() < source.len())
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
        let source_1 = "src/**".to_string();
        directory_overrider.process(&team_name_1, &source_1);
        assert_eq!(directory_overrider.specific_directory_owner(), Some((&team_name_1, &source_1)));

        let team_name_longest = "team2".to_string();
        let source_longest = "source/subdir/**".to_string();
        directory_overrider.process(&team_name_longest, &source_longest);
        assert_eq!(
            directory_overrider.specific_directory_owner(),
            Some((&team_name_longest, &source_longest))
        );

        let team_name_3 = "team3".to_string();
        let source_3 = "source/**".to_string();
        directory_overrider.process(&team_name_3, &source_3);
        assert_eq!(
            directory_overrider.specific_directory_owner(),
            Some((&team_name_longest, &source_longest))
        );
    }

    fn assert_owner_for(glob: &str, relative_path: &str, expect_match: bool) {
        let source = "directory_mapper (\"packs/bam\")".to_string();
        let team_name = "team1".to_string();
        let owner_matcher = OwnerMatcher::Glob {
            glob: glob.to_string(),
            team_name: team_name.clone(),
            source: source.clone(),
        };
        let response = owner_matcher.owner_for(&Path::new(relative_path));
        if expect_match {
            assert_eq!(response, (Some(&team_name), &source));
        } else {
            assert_eq!(response, (None, &source));
        }
    }

    #[test]
    fn owner_for_without_brackets_in_glob() {
        assert_owner_for("packs/bam/**/**", "packs/bam/app/components/sidebar.jsx", true);
        assert_owner_for("packs/bam/**/**", "packs/baz/app/components/sidebar.jsx", false);
        assert_owner_for("packs/bam/**/**", "packs/bam/app/[components]/gadgets/sidebar.jsx", true);
        assert_owner_for("packs/bam/**/**", "packs/bam/app/sidebar_[component].jsx", true);
    }

    #[test]
    fn owner_for_with_brackets_in_glob() {
        assert_owner_for(
            "packs/bam/app/\\[components\\]/**/**",
            "packs/bam/app/[components]/gadgets/sidebar.jsx",
            true,
        );
        assert_owner_for("packs/\\[bam\\]/**/**", "packs/[bam]/app/components/sidebar.jsx", true);
    }

    #[test]
    fn owner_for_with_multiple_brackets_in_glob() {
        assert_owner_for(
            "packs/\\[bam\\]/bar/\\[foo\\]/**/**",
            "packs/[bam]/bar/[foo]/app/components/sidebar.jsx",
            true,
        );
    }
}
