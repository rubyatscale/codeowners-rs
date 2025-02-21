use file_owner_finder::FileOwnerFinder;
use itertools::Itertools;
use mapper::{OwnerMatcher, Source, TeamName};
use std::{
    error::Error,
    fmt::{self, Display},
    path::Path,
    sync::Arc,
};
use tracing::{info, instrument};

mod file_generator;
mod file_owner_finder;
pub(crate) mod mapper;
mod validator;

use crate::{
    ownership::mapper::DirectoryMapper,
    project::{Project, Team},
};

pub use validator::Errors as ValidatorErrors;

use self::{
    file_generator::FileGenerator,
    mapper::{JavascriptPackageMapper, Mapper, RubyPackageMapper, TeamFileMapper, TeamGemMapper, TeamGlobMapper, TeamYmlMapper},
    validator::Validator,
};

pub struct Ownership {
    project: Arc<Project>,
}

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
            "Unowned".to_string()
        } else {
            self.sources
                .iter()
                .sorted_by_key(|source| source.to_string())
                .map(|source| source.to_string())
                .collect::<Vec<_>>()
                .join("\n- ")
        };

        write!(
            f,
            "Team: {}\nGithub Team: {}\nTeam YML: {}\nDescription:\n- {}",
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

impl Ownership {
    pub fn build(project: Project) -> Self {
        Self {
            project: Arc::new(project),
        }
    }

    #[instrument(level = "debug", skip_all)]
    pub fn validate(&self) -> Result<(), ValidatorErrors> {
        info!("validating file ownership");
        let validator = Validator {
            project: self.project.clone(),
            mappers: self.mappers(),
            file_generator: FileGenerator { mappers: self.mappers() },
        };

        validator.validate()
    }

    #[instrument(level = "debug", skip_all)]
    pub fn for_file(&self, file_path: &str) -> Result<Vec<FileOwner>, ValidatorErrors> {
        info!("getting file ownership for {}", file_path);
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
        let file_generator = FileGenerator { mappers: self.mappers() };
        file_generator.generate_file()
    }

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
}

fn parse_for_team(team_name: String, codeowners_file: &str) -> Result<Vec<TeamOwnership>, Box<dyn Error>> {
    let mut output = vec![];
    let mut current_section: Option<TeamOwnership> = None;
    let input: String = codeowners_file.replace(&FileGenerator::disclaimer().join("\n"), "");
    let error_message = "CODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file";

    for line in input.trim_start().lines() {
        match line {
            comment if comment.starts_with("#") => {
                if let Some(section) = current_section.take() {
                    output.push(section);
                }
                current_section = Some(TeamOwnership::new(comment.to_string()));
            }
            "" => {
                if let Some(section) = current_section.take() {
                    output.push(section);
                }
            }
            team_line if team_line.ends_with(&team_name) => {
                let section = current_section.as_mut().ok_or(error_message)?;

                let glob = line.split_once(' ').ok_or(error_message)?.0.to_string();
                section.globs.push(glob);
            }
            _ => {}
        }
    }

    if let Some(cs) = current_section {
        output.push(cs.clone());
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common_test::tests::{build_ownership_with_all_mappers, vecs_match};
    use indoc::indoc;

    #[test]
    fn test_for_file_owner() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let file_owners = ownership.for_file("app/consumers/directory_owned.rb").unwrap();
        assert_eq!(file_owners.len(), 1);
        assert_eq!(file_owners[0].team.name, "Bar");
        assert_eq!(file_owners[0].team_config_file_path, "config/teams/bar.yml");
        Ok(())
    }

    #[test]
    fn test_for_file_no_owner() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let file_owners = ownership.for_file("app/madeup/foo.rb").unwrap();
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

    #[test]
    fn test_parse_for_team_trims_header() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # STOP! - DO NOT EDIT THIS FILE MANUALLY
            # This file was automatically generated by \"bin/codeownership validate\".
            #
            # CODEOWNERS is used for GitHub to suggest code/file owners to various GitHub
            # teams. This is useful when developers create Pull Requests since the
            # code/file owner is notified. Reference GitHub docs for more details:
            # https://help.github.com/en/articles/about-code-owners


        "};

        let team_ownership = parse_for_team("@Bar".to_string(), codeownership_file)?;
        assert!(team_ownership.is_empty());
        Ok(())
    }

    #[test]
    fn test_parse_for_team_includes_owned_globs() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            /path/to/owned @Foo
            /path/to/not/owned @Bar

            # Last Section
            /another/owned/path @Foo
        "};

        let team_ownership = parse_for_team("@Foo".to_string(), codeownership_file)?;
        vecs_match(
            &team_ownership,
            &vec![
                TeamOwnership {
                    heading: "# First Section".to_string(),
                    globs: vec!["/path/to/owned".to_string()],
                },
                TeamOwnership {
                    heading: "# Last Section".to_string(),
                    globs: vec!["/another/owned/path".to_string()],
                },
            ],
        );
        Ok(())
    }

    #[test]
    fn test_parse_for_team_with_partial_team_match() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            /path/to/owned @Foo
            /path/to/not/owned @FooBar
        "};

        let team_ownership = parse_for_team("@Foo".to_string(), codeownership_file)?;
        vecs_match(
            &team_ownership,
            &vec![TeamOwnership {
                heading: "# First Section".to_string(),
                globs: vec!["/path/to/owned".to_string()],
            }],
        );
        Ok(())
    }

    #[test]
    fn test_parse_for_team_with_trailing_newlines() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            /path/to/owned @Foo

            # Last Section
            /another/owned/path @Foo



        "};

        let team_ownership = parse_for_team("@Foo".to_string(), codeownership_file)?;
        vecs_match(
            &team_ownership,
            &vec![
                TeamOwnership {
                    heading: "# First Section".to_string(),
                    globs: vec!["/path/to/owned".to_string()],
                },
                TeamOwnership {
                    heading: "# Last Section".to_string(),
                    globs: vec!["/another/owned/path".to_string()],
                },
            ],
        );
        Ok(())
    }

    #[test]
    fn test_parse_for_team_without_trailing_newline() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            /path/to/owned @Foo"};

        let team_ownership = parse_for_team("@Foo".to_string(), codeownership_file)?;
        vecs_match(
            &team_ownership,
            &vec![TeamOwnership {
                heading: "# First Section".to_string(),
                globs: vec!["/path/to/owned".to_string()],
            }],
        );
        Ok(())
    }

    #[test]
    fn test_parse_for_team_with_missing_section_header() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            /path/to/owned @Foo

            /another/owned/path @Foo
        "};

        let team_ownership = parse_for_team("@Foo".to_string(), codeownership_file);
        assert!(
            team_ownership
                .is_err_and(|e| e.to_string() == "CODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file")
        );
        Ok(())
    }

    #[test]
    fn test_parse_for_team_with_malformed_team_line() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            @Foo
        "};

        let team_ownership = parse_for_team("@Foo".to_string(), codeownership_file);
        assert!(
            team_ownership
                .is_err_and(|e| e.to_string() == "CODEOWNERS out of date. Run `codeowners generate` to update the CODEOWNERS file")
        );
        Ok(())
    }

    #[test]
    fn test_parse_for_team_with_invalid_file() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            # Second Section
            path/to/owned @Foo
        "};
        let team_ownership = parse_for_team("@Foo".to_string(), codeownership_file)?;
        vecs_match(
            &team_ownership,
            &vec![
                TeamOwnership {
                    heading: "# First Section".to_string(),
                    globs: vec![],
                },
                TeamOwnership {
                    heading: "# Second Section".to_string(),
                    globs: vec!["path/to/owned".to_string()],
                },
            ],
        );
        Ok(())
    }
}
