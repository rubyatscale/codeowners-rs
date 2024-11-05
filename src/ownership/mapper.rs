use glob_match::glob_match;
use std::{
    collections::HashMap,
    fmt::{self, Display},
    path::{Path, PathBuf},
};

pub(crate) mod directory_mapper;
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

#[derive(Debug, PartialEq, Clone)]
pub enum Source {
    Directory(String),
    TeamFile,
    TeamGem,
    TeamGlob,
    Package(String, String),
    TeamYml,
}

impl Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Source::Directory(path) => write!(f, "DirectoryMapper({})", path),
            Source::TeamFile => write!(f, "TeamFileMapper"),
            Source::TeamGem => write!(f, "TeamGemMapper"),
            Source::TeamGlob => write!(f, "TeamGlobMapper"),
            Source::Package(file_type, path) => write!(f, "PackageMapper({}, glob: {})", file_type, path),
            Source::TeamYml => write!(f, "TeamYmlMapper"),
        }
    }
}

impl Source {
    pub fn len(&self) -> usize {
        match self {
            Source::Directory(path) => path.matches('/').count(),
            _ => 0,
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_owner_for(glob: &str, relative_path: &str, expect_match: bool) {
        let source = Source::Directory("packs/bam".to_string());
        let team_name = "team1".to_string();
        let owner_matcher = OwnerMatcher::Glob {
            glob: glob.to_string(),
            team_name: team_name.clone(),
            source: source.clone(),
        };
        let response = owner_matcher.owner_for(Path::new(relative_path));
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

    #[test]
    fn display_source() {
        assert_eq!(Source::Directory("packs/bam".to_string()).to_string(), "DirectoryMapper(packs/bam)");
        assert_eq!(Source::TeamFile.to_string(), "TeamFileMapper");
        assert_eq!(Source::TeamGem.to_string(), "TeamGemMapper");
        assert_eq!(Source::TeamGlob.to_string(), "TeamGlobMapper");
        assert_eq!(
            Source::Package("Ruby".to_string(), "packs/bam/**/**".to_string()).to_string(),
            "PackageMapper(Ruby, glob: packs/bam/**/**)"
        );
        assert_eq!(Source::TeamYml.to_string(), "TeamYmlMapper");
    }
}
