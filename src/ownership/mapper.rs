use fast_glob::glob_match;
use std::{
    collections::HashMap,
    fmt::{self, Display},
    path::{Path, PathBuf},
};

pub(crate) mod directory_mapper;
mod escaper;
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
    AnnotatedFile,
    TeamGem,
    TeamGlob(String),
    Package(String, String),
    TeamYml,
}

impl Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Source::Directory(path) => write!(f, "Owner specified in `{}/.codeowner`", path),
            Source::AnnotatedFile => write!(f, "Owner annotation at the top of the file"),
            Source::TeamGem => write!(f, "Owner specified in Team YML's `owned_gems`"),
            Source::TeamGlob(glob) => write!(f, "Owner specified in Team YML as an owned_glob `{}`", glob),
            Source::Package(package_path, glob) => {
                write!(f, "Owner defined in `{}` with implicity owned glob: `{}`", package_path, glob)
            }
            Source::TeamYml => write!(f, "Teams own their configuration files"),
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
    Glob {
        glob: String,
        subtracted_globs: Vec<String>,
        team_name: TeamName,
        source: Source,
    },
}

impl OwnerMatcher {
    pub fn new_glob_with_candidate_subtracted_globs(
        glob: String,
        candidate_subtracted_globs: &[String],
        team_name: TeamName,
        source: Source,
    ) -> Self {
        let subtracted_globs = candidate_subtracted_globs
            .iter()
            .filter(|candidate_subtracted_glob| {
                glob_match(candidate_subtracted_glob, &glob) || glob_match(&glob, candidate_subtracted_glob)
            })
            .cloned()
            .collect();
        OwnerMatcher::Glob {
            glob,
            subtracted_globs,
            team_name,
            source,
        }
    }

    pub fn new_glob(glob: String, team_name: TeamName, source: Source) -> Self {
        OwnerMatcher::Glob {
            glob,
            subtracted_globs: vec![],
            team_name,
            source,
        }
    }

    pub fn owner_for(&self, relative_path: &Path) -> (Option<&TeamName>, &Source) {
        match self {
            OwnerMatcher::Glob {
                glob,
                subtracted_globs,
                team_name,
                source,
            } => relative_path
                .to_str()
                .filter(|path| glob_match(glob, path) && !subtracted_globs.iter().any(|subtracted| glob_match(subtracted, path)))
                .map_or((None, source), |_| (Some(team_name), source)),
            OwnerMatcher::ExactMatches(path_to_team, source) => (path_to_team.get(relative_path), source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_owner_for(glob: &str, subtracted_globs: &[&str], relative_path: &str, expect_match: bool) {
        let source = Source::Directory("packs/bam".to_string());
        let team_name = "team1".to_string();
        let owner_matcher = OwnerMatcher::new_glob_with_candidate_subtracted_globs(
            glob.to_string(),
            &subtracted_globs.iter().map(|s| s.to_string()).collect::<Vec<String>>(),
            team_name.clone(),
            source.clone(),
        );
        let response = owner_matcher.owner_for(Path::new(relative_path));
        if expect_match {
            assert_eq!(response, (Some(&team_name), &source));
        } else {
            assert_eq!(response, (None, &source));
        }
    }

    #[test]
    fn owner_for_without_brackets_in_glob() {
        assert_owner_for("packs/bam/**/**", &[], "packs/bam/app/components/sidebar.jsx", true);
        assert_owner_for("packs/bam/**/**", &[], "packs/baz/app/components/sidebar.jsx", false);
        assert_owner_for("packs/bam/**/**", &[], "packs/bam/app/[components]/gadgets/sidebar.jsx", true);
        assert_owner_for("packs/bam/**/**", &[], "packs/bam/app/sidebar_[component].jsx", true);
        assert_owner_for(
            "packs/bam/**/**",
            &["packs/bam/app/excluded/**"],
            "packs/bam/app/excluded/sidebar_[component].jsx",
            false,
        );
    }

    #[test]
    fn subtracted_globs() {
        assert_owner_for(
            "packs/bam/**/**",
            &["packs/bam/app/excluded/**"],
            "packs/bam/app/excluded/some_file.rb",
            false,
        );
        assert_owner_for(
            "packs/bam/**/**",
            &["packs/bam/app/excluded/**"],
            "packs/bam/app/not_excluded/some_file.rb",
            true,
        );
    }

    #[test]
    fn owner_for_with_brackets_in_glob() {
        assert_owner_for(
            "packs/bam/app/\\[components\\]/**/**",
            &[],
            "packs/bam/app/[components]/gadgets/sidebar.jsx",
            true,
        );
        assert_owner_for("packs/\\[bam\\]/**/**", &[], "packs/[bam]/app/components/sidebar.jsx", true);
    }

    #[test]
    fn owner_for_with_multiple_brackets_in_glob() {
        assert_owner_for(
            "packs/\\[bam\\]/bar/\\[foo\\]/**/**",
            &[],
            "packs/[bam]/bar/[foo]/app/components/sidebar.jsx",
            true,
        );
    }

    #[test]
    fn display_source() {
        assert_eq!(
            Source::Directory("packs/bam".to_string()).to_string(),
            "Owner specified in `packs/bam/.codeowner`"
        );
        assert_eq!(Source::AnnotatedFile.to_string(), "Owner annotation at the top of the file");
        assert_eq!(Source::TeamGem.to_string(), "Owner specified in Team YML's `owned_gems`");
        assert_eq!(
            Source::TeamGlob("a/glob/**".to_string()).to_string(),
            "Owner specified in Team YML as an owned_glob `a/glob/**`"
        );
        assert_eq!(
            Source::Package("packs/bam/packag.yml".to_string(), "packs/bam/**/**".to_string()).to_string(),
            "Owner defined in `packs/bam/packag.yml` with implicity owned glob: `packs/bam/**/**`"
        );
        assert_eq!(Source::TeamYml.to_string(), "Teams own their configuration files");
    }

    #[test]
    fn test_new_glob_with_candidate_subtracted_globs() {
        assert_new_glob_with_candidate_subtracted_globs("packs/bam/**/**", &[], &[]);
        assert_new_glob_with_candidate_subtracted_globs("packs/bam/**/**", &["packs/bam/app/**/**"], &["packs/bam/app/**/**"]);
        assert_new_glob_with_candidate_subtracted_globs(
            "packs/bam/**/**",
            &["packs/bam/app/an/exceptional/path/it.rb"],
            &["packs/bam/app/an/exceptional/path/it.rb"],
        );
        assert_new_glob_with_candidate_subtracted_globs("packs/bam/**/**", &["packs/bam.rb"], &[]);
        assert_new_glob_with_candidate_subtracted_globs("packs/bam/**/**", &["packs/nope/app/**/**"], &[]);
        assert_new_glob_with_candidate_subtracted_globs("packs/**", &["packs/yep/app/**/**"], &["packs/yep/app/**/**"]);
        assert_new_glob_with_candidate_subtracted_globs("packs/foo.yml", &["packs/foo/**/**"], &[]);
    }

    fn assert_new_glob_with_candidate_subtracted_globs(
        glob: &str,
        candidate_subtracted_globs: &[&str],
        expected_subtracted_globs: &[&str],
    ) {
        let owner_matcher = OwnerMatcher::new_glob_with_candidate_subtracted_globs(
            glob.to_string(),
            &candidate_subtracted_globs.iter().map(|s| s.to_string()).collect::<Vec<String>>(),
            "team1".to_string(),
            Source::TeamGlob(glob.to_string()),
        );

        if let OwnerMatcher::Glob { subtracted_globs, .. } = owner_matcher {
            assert_eq!(subtracted_globs, expected_subtracted_globs);
        } else {
            panic!("Expected a Glob matcher");
        }
    }
}
