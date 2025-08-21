use crate::{
    ownership::{FileGenerator, TeamOwnership},
    project::Team,
};
use fast_glob::glob_match;
use memoize::memoize;
use rayon::prelude::*;
use regex::Regex;
use std::{
    collections::HashMap,
    error::Error,
    fs,
    io::Error as IoError,
    path::{Path, PathBuf},
};

use super::file_generator::compare_lines;

pub struct Parser {
    pub project_root: PathBuf,
    pub codeowners_file_path: PathBuf,
    pub team_file_globs: Vec<String>,
}

impl Parser {
    pub fn teams_from_files_paths(&self, file_paths: &[PathBuf]) -> Result<HashMap<String, Option<Team>>, Box<dyn Error>> {
        let file_inputs: Vec<(String, String)> = file_paths
            .iter()
            .map(|path| {
                let file_path_str = path
                    .to_str()
                    .ok_or(IoError::new(std::io::ErrorKind::InvalidInput, "Invalid file path"))?;
                let original = file_path_str.to_string();
                let prefixed = if file_path_str.starts_with('/') {
                    original.clone()
                } else {
                    format!("/{}", file_path_str)
                };
                Ok((original, prefixed))
            })
            .collect::<Result<Vec<_>, IoError>>()?;

        if file_inputs.is_empty() {
            return Ok(HashMap::new());
        }

        let codeowners_entries: Vec<(String, String)> =
            build_codeowners_lines_in_priority(self.codeowners_file_path.to_string_lossy().into_owned())
                .iter()
                .map(|line| {
                    line.split_once(' ')
                        .map(|(glob, team_name)| (glob.to_string(), team_name.to_string()))
                        .ok_or_else(|| IoError::new(std::io::ErrorKind::InvalidInput, "Invalid line"))
                })
                .collect::<Result<_, IoError>>()
                .map_err(|e| Box::new(e) as Box<dyn Error>)?;

        let teams_by_name = teams_by_github_team_name(self.absolute_team_files_globs());

        let result: HashMap<String, Option<Team>> = file_inputs
            .par_iter()
            .map(|(key, prefixed)| {
                let team = codeowners_entries
                    .iter()
                    .find(|(glob, _)| glob_match(glob, prefixed))
                    .and_then(|(_, team_name)| teams_by_name.get(team_name).cloned());
                (key.clone(), team)
            })
            .collect();

        Ok(result)
    }

    pub fn team_from_file_path(&self, file_path: &Path) -> Result<Option<Team>, Box<dyn Error>> {
        let teams = self.teams_from_files_paths(&[file_path.to_path_buf()])?;
        Ok(teams.get(file_path.to_string_lossy().into_owned().as_str()).cloned().flatten())
    }

    fn absolute_team_files_globs(&self) -> Vec<String> {
        self.team_file_globs
            .iter()
            .map(|glob| format!("{}/{}", self.project_root.display(), glob))
            .collect()
    }
}

#[memoize]
fn teams_by_github_team_name(team_file_glob: Vec<String>) -> HashMap<String, Team> {
    let mut teams = HashMap::new();
    for glob in team_file_glob {
        match glob::glob(&glob) {
            Ok(paths) => {
                for path in paths.filter_map(Result::ok) {
                    let team = match Team::from_team_file_path(path) {
                        Ok(team) => team,
                        Err(e) => {
                            eprintln!("Error parsing team file: {}", e);
                            continue;
                        }
                    };
                    teams.insert(team.github_team.clone(), team);
                }
            }
            Err(e) => {
                eprintln!("Failed to read glob pattern '{}': {}", glob, e);
                continue;
            }
        }
    }

    teams
}

#[memoize]
fn build_codeowners_lines_in_priority(codeowners_file_path: String) -> Vec<String> {
    let codeowners_file = match fs::read_to_string(codeowners_file_path) {
        Ok(codeowners_file) => codeowners_file,
        Err(e) => {
            // we can't return the error because it's not clonable
            eprintln!("Error reading codeowners file: {}", e);
            return vec![];
        }
    };
    stripped_lines_by_priority(&codeowners_file)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Section {
    heading: String,
    lines: Vec<String>,
}

impl Section {
    fn new(heading: String, lines: Vec<String>) -> Self {
        let mut sorted_lines = lines.clone();
        sorted_lines.sort_by(compare_lines);
        Self {
            heading,
            lines: sorted_lines,
        }
    }
}

fn codeowner_sections(codeowners_file: &str) -> Result<Vec<Section>, Box<dyn Error>> {
    let un_ignore = Regex::new(r"^# \/")?;
    let mut iter = codeowners_file.lines().peekable();
    let mut sections = Vec::new();
    let mut current_section = None;
    let mut current_lines = Vec::new();

    while let Some(line) = iter.next() {
        let line = un_ignore.replace(line, "/").to_string();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('#') {
            if iter
                .peek()
                .map(|next| next.starts_with('/') || next.starts_with("# /"))
                .unwrap_or(false)
            {
                if let Some(section_name) = current_section.take() {
                    sections.push(Section::new(section_name, std::mem::take(&mut current_lines)));
                }
                current_section = Some(line);
            }
        } else {
            current_lines.push(line);
        }
    }

    if let Some(section_name) = current_section {
        sections.push(Section::new(section_name, current_lines));
    }

    Ok(sections)
}

fn stripped_lines_by_priority(codeowners_file: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let sections = codeowner_sections(codeowners_file).unwrap_or_default();
    for section in sections {
        lines.extend(section.lines);
    }
    lines.reverse();
    lines
}

pub fn parse_for_team(team_name: String, codeowners_file: &str) -> Result<Vec<TeamOwnership>, Box<dyn Error>> {
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

    use crate::common_test::tests::vecs_match;

    use super::*;
    use indoc::indoc;

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

    #[test]
    fn test_stripped_lines_by_priority() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            /path/to/owned @Foo
        "};

        let stripped_lines = stripped_lines_by_priority(codeownership_file);
        assert_eq!(stripped_lines, vec!["/path/to/owned @Foo"]);
        Ok(())
    }

    #[test]
    fn test_stripped_lines_by_priority_with_multiple_sections() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # First Section
            /path/to/owned @Foo

            # Second Section
            /another/path/to/owned @Bar
        "};

        let stripped_lines = stripped_lines_by_priority(codeownership_file);
        assert_eq!(stripped_lines, vec!["/another/path/to/owned @Bar", "/path/to/owned @Foo"]);
        Ok(())
    }

    #[test]
    fn test_stripped_lines_by_priority_with_ignored_teams() -> Result<(), Box<dyn Error>> {
        let codeownership_file = indoc! {"
            # STOP! - DO NOT EDIT THIS FILE MANUALLY
            # This file was automatically generated by \"bin/codeownership validate\".
            #
            # CODEOWNERS is used for GitHub to suggest code/file owners to various GitHub
            # teams. This is useful when developers create Pull Requests since the
            # code/file owner is notified. Reference GitHub docs for more details:
            # https://help.github.com/en/articles/about-code-owners


            # Annotations at the top of file
            # /app/assets/config/manifest.js @Prefix/team-foo
            # /config/application.rb @Prefix/team-bar
            /config/i18n-tasks.yml.erb @Prefix/language-team

            # Team-specific owned globs
            # /.github/workflows/pull-translations.yml @Prefix/infra
            # /.github/workflows/push-sources.yml @Prefix/infra
            # /Dockerfile @Prefix/docker-team
            # /components/create.rb @Prefix/component-team
            /.codeclimate.yml @Prefix/climate-team
            /.vscode/extensions/z/**/* @Prefix/zteam
            /bin/brakeman @Prefix/psecurity
            /config/brakeman.ignore @Prefix/security
        "};

        // build up each sections lines
        // resort the lines without the '#'
        // re-assemble the sections
        // reverse sort
        let codeowner_sections = codeowner_sections(codeownership_file)?;
        assert_eq!(
            codeowner_sections,
            vec![
                Section {
                    heading: "# Annotations at the top of file".to_string(),
                    lines: vec![
                        "/app/assets/config/manifest.js @Prefix/team-foo".to_string(),
                        "/config/application.rb @Prefix/team-bar".to_string(),
                        "/config/i18n-tasks.yml.erb @Prefix/language-team".to_string()
                    ]
                },
                Section {
                    heading: "# Team-specific owned globs".to_string(),
                    lines: vec![
                        "/.codeclimate.yml @Prefix/climate-team".to_string(),
                        "/.github/workflows/pull-translations.yml @Prefix/infra".to_string(),
                        "/.github/workflows/push-sources.yml @Prefix/infra".to_string(),
                        "/.vscode/extensions/z/**/* @Prefix/zteam".to_string(),
                        "/Dockerfile @Prefix/docker-team".to_string(),
                        "/bin/brakeman @Prefix/psecurity".to_string(),
                        "/components/create.rb @Prefix/component-team".to_string(),
                        "/config/brakeman.ignore @Prefix/security".to_string()
                    ]
                },
            ]
        );
        let stripped_lines = stripped_lines_by_priority(codeownership_file);
        assert_eq!(
            stripped_lines,
            vec![
                "/config/brakeman.ignore @Prefix/security",
                "/components/create.rb @Prefix/component-team",
                "/bin/brakeman @Prefix/psecurity",
                "/Dockerfile @Prefix/docker-team",
                "/.vscode/extensions/z/**/* @Prefix/zteam",
                "/.github/workflows/push-sources.yml @Prefix/infra",
                "/.github/workflows/pull-translations.yml @Prefix/infra",
                "/.codeclimate.yml @Prefix/climate-team",
                "/config/i18n-tasks.yml.erb @Prefix/language-team",
                "/config/application.rb @Prefix/team-bar",
                "/app/assets/config/manifest.js @Prefix/team-foo"
            ]
        );
        Ok(())
    }

    #[test]
    fn test_unignore_regex() -> Result<(), Box<dyn Error>> {
        let un_ignore = Regex::new(r"^# \/")?;
        assert_eq!(un_ignore.replace("# /path/to/owned", "/"), "/path/to/owned");
        Ok(())
    }
}
