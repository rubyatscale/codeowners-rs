use file_owner_finder::FileOwnerFinder;
use itertools::Itertools;
use mapper::{OwnerMatcher, Source, TeamName};
use std::{
    error::Error,
    fmt::{self, Display},
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{info, instrument};

pub(crate) mod codeowners_file_parser;
pub(crate) mod codeowners_query;
mod file_generator;
mod file_owner_finder;
pub mod file_owner_resolver;
pub(crate) mod mapper;
mod validator;

use crate::{
    ownership::mapper::DirectoryMapper,
    project::{Project, Team},
};

pub use validator::Errors as ValidatorErrors;

use self::{
    codeowners_file_parser::parse_for_team,
    file_generator::FileGenerator,
    mapper::{JavascriptPackageMapper, Mapper, RubyPackageMapper, TeamFileMapper, TeamGemMapper, TeamGlobMapper, TeamYmlMapper},
    validator::Validator,
};

pub struct Ownership {
    project: Arc<Project>,
}
#[derive(Debug, Clone)]
pub struct FileOwner {
    pub team: Team,
    pub team_config_file_path: String,
    pub sources: Vec<Source>,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct TeamOwnership {
    pub heading: String,
    pub globs: Vec<String>,
}

impl TeamOwnership {
    fn new(heading: String) -> Self {
        Self {
            heading,
            ..Default::default()
        }
    }
}

impl Display for FileOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sources = if self.sources.is_empty() {
            "".to_string()
        } else {
            let sources_str = self
                .sources
                .iter()
                .sorted_by_key(|source| source.to_string())
                .map(|source| source.to_string())
                .collect::<Vec<_>>()
                .join("\n- ");
            format!("\n- {}", sources_str)
        };

        write!(
            f,
            "Team: {}\nGithub Team: {}\nTeam YML: {}\nDescription:{}",
            self.team.name, self.team.github_team, self.team_config_file_path, sources
        )
    }
}

impl Default for FileOwner {
    fn default() -> Self {
        Self {
            team: Team {
                name: "Unowned".to_string(),
                github_team: "Unowned".to_string(),
                ..Default::default()
            },
            team_config_file_path: "".to_string(),
            sources: vec![],
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub github_team: String,
    pub team_name: TeamName,
    pub disabled: bool,
}

impl Entry {
    fn to_row(&self) -> String {
        let line = format!("/{} {}", self.path, self.github_team);
        if self.disabled { format!("# {}", line) } else { line }
    }
}

/// Tells whether anything names an owner for a project-relative path, even a team that doesn't exist: an annotation naming one
/// still counts for its file, and a `.codeowner`, `package.yml` or `package.json` naming one for every path under its directory.
/// It sees only files the project reads, so an annotated file, `.codeowner` or package file names no owner here if it is
/// untracked or under a top-level directory named in ignore_dirs, as in `validate`. Building one goes through every project
/// file, so reuse it.
pub(crate) struct OwnerLookup {
    owner_matchers: Vec<OwnerMatcher>,
    unknown_team_roots: Vec<PathBuf>,
}

impl OwnerLookup {
    pub(crate) fn has_owner(&self, relative_path: &Path) -> bool {
        // FileOwnerFinder::find returns an owner exactly when some matcher names a team, but builds every owner to do it.
        self.unknown_team_roots.iter().any(|root| relative_path.starts_with(root))
            || self
                .owner_matchers
                .iter()
                .any(|owner_matcher| owner_matcher.owner_for(relative_path).0.is_some())
    }
}

impl Ownership {
    pub fn build(project: Project) -> Self {
        Self {
            project: Arc::new(project),
        }
    }

    #[instrument(name = "ownership_validate", level = "debug", skip_all)]
    pub fn validate(&self) -> Result<(), ValidatorErrors> {
        info!("validating file ownership");
        let validator = Validator {
            project: self.project.clone(),
            mappers: self.mappers(),
            file_generator: FileGenerator {
                mappers: self.codeowners_file_mappers(),
            },
            executable_name: self.project.executable_name.clone(),
        };

        validator.validate()
    }

    #[instrument(level = "debug", skip_all)]
    pub fn for_file(&self, file_path: &Path) -> Result<Vec<FileOwner>, ValidatorErrors> {
        info!("getting file ownership for {}", file_path.display());
        let owner_matchers: Vec<OwnerMatcher> = self.mappers().iter().flat_map(|mapper| mapper.owner_matchers()).collect();
        let file_owner_finder = FileOwnerFinder {
            owner_matchers: &owner_matchers,
        };
        let owners = file_owner_finder.find(Path::new(file_path));
        Ok(owners
            .iter()
            .sorted_by_key(|owner| owner.team_name.to_lowercase())
            .map(|owner| match self.project.get_team(&owner.team_name) {
                Some(team) => FileOwner {
                    team: team.clone(),
                    team_config_file_path: team
                        .path
                        .strip_prefix(&self.project.base_path)
                        .map_or_else(|_| String::new(), |p| p.to_string_lossy().to_string()),
                    sources: owner.sources.clone(),
                },
                None => FileOwner::default(),
            })
            .collect())
    }

    pub(crate) fn owner_lookup(&self) -> OwnerLookup {
        // `validate` reports a package.yml, package.json or `.codeowner` naming a team that doesn't exist, but a list of files
        // isn't checked for that, so here each still names an owner for the files under its directory. The package mappers drop
        // such a package, and the directory mapper's glob misses a `.codeowner` at the project root and reads braces as alternatives,
        // matching `app/legacy` for one in `app/{legacy}`. So the directory is matched as a path prefix, which replaces that glob
        // for such a `.codeowner`: brackets and braces are literal and the root covers every file.
        let package_roots = self
            .project
            .packages
            .iter()
            .filter(|package| !self.project.teams_by_name.contains_key(&package.owner))
            .filter_map(|package| package.package_root());
        let directory_roots = self
            .project
            .directory_codeowner_files
            .iter()
            .filter(|directory_codeowner_file| !self.project.teams_by_name.contains_key(&directory_codeowner_file.owner))
            .filter_map(|directory_codeowner_file| directory_codeowner_file.directory_root());
        OwnerLookup {
            owner_matchers: self
                .mappers()
                .iter()
                .flat_map(|mapper| mapper.owner_matchers())
                .filter(|owner_matcher| match owner_matcher {
                    OwnerMatcher::Glob {
                        source: Source::Directory(_),
                        team_name,
                        ..
                    } => self.project.teams_by_name.contains_key(team_name),
                    _ => true,
                })
                .collect(),
            unknown_team_roots: package_roots.chain(directory_roots).map(Path::to_path_buf).collect(),
        }
    }

    #[instrument(level = "debug", skip_all)]
    pub fn for_team(&self, team_name: &str) -> Result<Vec<TeamOwnership>, Box<dyn Error>> {
        info!("getting team ownership for {}", team_name);
        let team = self.project.get_team(team_name).ok_or("Team not found")?;
        let codeowners_file = self.project.get_codeowners_file()?;

        parse_for_team(team.github_team, &codeowners_file)
    }

    #[instrument(level = "debug", skip_all)]
    pub fn generate_file(&self) -> String {
        info!("generating codeowners file");
        let file_generator = FileGenerator {
            mappers: self.codeowners_file_mappers(),
        };
        file_generator.generate_file()
    }

    #[instrument(name = "mapper_build", level = "debug", skip_all)]
    fn mappers(&self) -> Vec<Box<dyn Mapper>> {
        vec![
            Box::new(TeamFileMapper::build(self.project.clone())),
            Box::new(TeamGlobMapper::build(self.project.clone())),
            Box::new(DirectoryMapper::build(self.project.clone())),
            Box::new(RubyPackageMapper::build(self.project.clone())),
            Box::new(JavascriptPackageMapper::build(self.project.clone())),
            Box::new(TeamYmlMapper::build(self.project.clone())),
            Box::new(TeamGemMapper::build(self.project.clone())),
        ]
    }

    // GitHub applies the last matching CODEOWNERS line, so annotations are written last: an annotated
    // file can still match a broader team glob that excludes it through unowned_globs.
    fn codeowners_file_mappers(&self) -> Vec<Box<dyn Mapper>> {
        vec![
            Box::new(TeamGlobMapper::build(self.project.clone())),
            Box::new(DirectoryMapper::build(self.project.clone())),
            Box::new(RubyPackageMapper::build(self.project.clone())),
            Box::new(JavascriptPackageMapper::build(self.project.clone())),
            Box::new(TeamYmlMapper::build(self.project.clone())),
            Box::new(TeamGemMapper::build(self.project.clone())),
            Box::new(TeamFileMapper::build(self.project.clone())),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_test::tests::build_ownership_with_all_mappers;

    #[test]
    fn test_for_file_owner() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let file_owners = ownership.for_file(Path::new("app/consumers/directory_owned.rb")).unwrap();
        assert_eq!(file_owners.len(), 1);
        assert_eq!(file_owners[0].team.name, "Bar");
        assert_eq!(file_owners[0].team_config_file_path, "config/teams/bar.yml");
        Ok(())
    }

    #[test]
    fn test_for_file_no_owner() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let file_owners = ownership.for_file(Path::new("app/madeup/foo.rb")).unwrap();
        assert_eq!(file_owners.len(), 0);
        Ok(())
    }

    #[test]
    fn test_for_team() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let team_ownership = ownership.for_team("Bar");
        assert!(team_ownership.is_ok());
        Ok(())
    }

    #[test]
    fn test_for_team_not_found() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let team_ownership = ownership.for_team("Nope");
        assert!(team_ownership.is_err(), "Team not found");
        Ok(())
    }
}
