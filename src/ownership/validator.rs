use crate::path_utils::matches_globs;
use crate::project::{Project, ProjectFile};
use core::fmt;
use std::collections::HashSet;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use itertools::Itertools;
use rayon::prelude::IntoParallelRefIterator;
use rayon::prelude::ParallelIterator;
use similar::{ChangeTag, TextDiff};
use tracing::debug;
use tracing::instrument;

use super::file_generator::FileGenerator;
use super::file_owner_finder::FileOwnerFinder;
use super::file_owner_finder::Owner;
use super::mapper::{Mapper, OwnerMatcher, TeamName};

pub struct Validator {
    pub project: Arc<Project>,
    pub mappers: Vec<Box<dyn Mapper>>,
    pub file_generator: FileGenerator,
    pub executable_name: String,
}

#[derive(Debug)]
enum Error {
    InvalidTeam {
        name: String,
        path: PathBuf,
    },
    /// A `.codeowner` naming a team that isn't registered. Distinct from `InvalidTeam` only
    /// so the message can name the ancestor the directory now silently inherits from, which
    /// is the part a reader needs in order to understand what happened. `category()`
    /// deliberately matches `InvalidTeam` so one typo'd team name across an annotation, a
    /// `package.yml`, and a `.codeowner` still groups under a single headline.
    InvalidDirectoryTeam {
        name: String,
        path: PathBuf,
        inherits_from: Option<PathBuf>,
    },
    FileWithoutOwner {
        path: PathBuf,
    },
    FileWithMultipleOwners {
        path: PathBuf,
        owners: Vec<Owner>,
    },
    CodeownershipFileIsStale {
        executable_name: String,
        diff: String,
    },
}

#[derive(Debug)]
pub struct Errors(Vec<Error>);

impl Validator {
    #[instrument(name = "validator_validate", level = "debug", skip_all)]
    pub fn validate(&self) -> Result<(), Errors> {
        let mut validation_errors = Vec::new();

        debug!("validate_invalid_team");
        validation_errors.append(&mut self.validate_invalid_team());

        debug!("validate_file_ownership");
        validation_errors.append(&mut self.validate_file_ownership());

        debug!("validate_codeowners_file");
        validation_errors.append(&mut self.validate_codeowners_file());

        if validation_errors.is_empty() {
            Ok(())
        } else {
            Err(Errors(validation_errors))
        }
    }

    #[instrument(name = "validate_invalid_team", level = "debug", skip_all)]
    fn validate_invalid_team(&self) -> Vec<Error> {
        debug!("validating project");
        let mut errors: Vec<Error> = Vec::new();

        let team_names: HashSet<&TeamName> = self.project.teams.iter().map(|team| &team.name).collect();

        errors.append(&mut self.invalid_team_annotation(&team_names));
        errors.append(&mut self.invalid_package_ownership(&team_names));
        errors.append(&mut self.invalid_directory_ownership());

        errors
    }

    fn invalid_team_annotation(&self, team_names: &HashSet<&String>) -> Vec<Error> {
        let project = self.project.clone();

        self.project
            .files
            .par_iter()
            .flat_map(|file| {
                if let Some(owner) = &file.owner
                    && !team_names.contains(owner)
                {
                    return Some(Error::InvalidTeam {
                        name: owner.clone(),
                        path: project.relative_path(&file.path).to_owned(),
                    });
                }

                None
            })
            .collect()
    }

    fn invalid_package_ownership(&self, team_names: &HashSet<&String>) -> Vec<Error> {
        self.project
            .packages
            .iter()
            .flat_map(|package| {
                if !team_names.contains(&package.owner) {
                    Some(Error::InvalidTeam {
                        name: package.owner.clone(),
                        path: self.project.relative_path(&package.path).to_owned(),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    /// `DirectoryMapper::entries` skips unresolvable owners, so the directory silently
    /// inherits its ancestor's owner and nothing else reports the bad name.
    ///
    /// Resolves through `teams_by_name` rather than `teams[].name` so the predicate is
    /// identical to the mapper's lookup: that map is keyed by both `name` and
    /// `github_team`, and a `.codeowner` holding either one generates a correct line.
    fn invalid_directory_ownership(&self) -> Vec<Error> {
        let resolvable_roots: HashSet<&Path> = self
            .project
            .directory_codeowner_files
            .iter()
            .filter(|directory_codeowner_file| self.project.teams_by_name.contains_key(&directory_codeowner_file.owner))
            .filter_map(|directory_codeowner_file| directory_codeowner_file.directory_root())
            .collect();

        self.project
            .directory_codeowner_files
            .iter()
            .flat_map(|directory_codeowner_file| {
                if !self.project.teams_by_name.contains_key(&directory_codeowner_file.owner) {
                    Some(Error::InvalidDirectoryTeam {
                        name: directory_codeowner_file.owner.clone(),
                        path: self.project.relative_path(&directory_codeowner_file.path).to_owned(),
                        inherits_from: directory_codeowner_file
                            .directory_root()
                            .and_then(|root| root.ancestors().skip(1).find(|ancestor| resolvable_roots.contains(ancestor)))
                            .map(|ancestor| self.project.relative_path(ancestor).to_owned()),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    #[instrument(name = "validate_file_ownership", level = "debug", skip_all)]
    fn validate_file_ownership(&self) -> Vec<Error> {
        let mut validation_errors = Vec::new();

        for (file, owners) in self.file_to_owners() {
            let relative_path = self.project.relative_path(&file.path).to_owned();

            if owners.is_empty() {
                if !matches_globs(&relative_path, &self.project.allow_unowned_globs) {
                    validation_errors.push(Error::FileWithoutOwner { path: relative_path })
                }
            } else if owners.len() > 1 {
                validation_errors.push(Error::FileWithMultipleOwners {
                    path: relative_path,
                    owners,
                })
            }
        }

        validation_errors
    }

    #[instrument(name = "validate_codeowners_file", level = "debug", skip_all)]
    fn validate_codeowners_file(&self) -> Vec<Error> {
        let generated_file = self.file_generator.generate_file();
        let current_file = self.project.get_codeowners_file().unwrap_or_default();

        if generated_file == current_file {
            vec![]
        } else {
            vec![Error::CodeownershipFileIsStale {
                executable_name: self.executable_name.to_string(),
                diff: codeowners_diff(&current_file, &generated_file),
            }]
        }
    }

    #[instrument(name = "file_to_owners", level = "debug", skip_all)]
    fn file_to_owners(&self) -> Vec<(&ProjectFile, Vec<Owner>)> {
        let owner_matchers: Vec<OwnerMatcher> = self.mappers.iter().flat_map(|mapper| mapper.owner_matchers()).collect();
        let file_owner_finder = FileOwnerFinder {
            owner_matchers: &owner_matchers,
        };
        let project = self.project.clone();

        self.project
            .files
            .par_iter()
            .filter_map(|project_file| {
                let relative_path = project.relative_path(&project_file.path);
                let owners = file_owner_finder.find(relative_path);
                Some((project_file, owners))
            })
            .collect()
    }
}

/// Builds a line-oriented diff between the current (on-disk) CODEOWNERS file and the
/// freshly generated one, so that validation failures explain *what* is out of date
/// rather than just *that* it is. Only changed lines are emitted: removals (present
/// on disk but no longer expected) are prefixed with `-` and additions (expected but
/// missing) are prefixed with `+`.
fn codeowners_diff(current: &str, generated: &str) -> String {
    let diff = TextDiff::from_lines(current, generated);

    diff.iter_all_changes()
        .filter_map(|change| {
            let line = change.value().trim_end_matches('\n');
            match change.tag() {
                ChangeTag::Delete => Some(format!("-{line}")),
                ChangeTag::Insert => Some(format!("+{line}")),
                ChangeTag::Equal => None,
            }
        })
        .join("\n")
}

impl Error {
    pub fn category(&self) -> String {
        match self {
                Error::FileWithoutOwner { path: _ } => "Some files are missing ownership".to_owned(),
                Error::FileWithMultipleOwners { path: _, owners: _ } => "Code ownership should only be defined for each file in one way. The following files have declared ownership in multiple ways".to_owned(),
                Error::CodeownershipFileIsStale { executable_name, diff: _ } => {
                    format!("CODEOWNERS out of date. Run `{}` to update the CODEOWNERS file", executable_name)
                }
                Error::InvalidTeam { name: _, path: _ } => "Found invalid team references".to_owned(),
                Error::InvalidDirectoryTeam { .. } => "Found invalid team references".to_owned(),
            }
    }

    pub fn messages(&self) -> Vec<String> {
        match self {
            Error::FileWithoutOwner { path } => vec![format!("- {}", path.to_string_lossy())],
            Error::FileWithMultipleOwners { path, owners } => {
                let path_display = path.to_string_lossy();
                let mut messages = vec![format!("\n{path_display}")];

                owners
                    .iter()
                    .sorted_by_key(|owner| owner.team_name.to_lowercase())
                    .for_each(|owner| {
                        messages.push(format!(" owner: {}", owner.team_name));
                        messages.extend(owner.sources.iter().map(|source| format!("  - {source}")));
                    });

                vec![messages.join("\n")]
            }
            // The diff is intentionally *not* rendered as part of the error. It is
            // surfaced separately as an informational message (see `Errors::info_messages`)
            // so that a long diff doesn't bury the actionable headline.
            Error::CodeownershipFileIsStale { .. } => vec![],
            Error::InvalidTeam { name, path } => vec![format!("- {} is referencing an invalid team - '{}'", path.to_string_lossy(), name)],
            Error::InvalidDirectoryTeam { name, path, inherits_from } => {
                // `owner` is `content.trim()`, so an empty or whitespace-only file yields "".
                // Reporting that as `an invalid team - ''` gives no hint the file is empty.
                let mut message = if name.is_empty() {
                    format!("- {} is empty and names no team", path.to_string_lossy())
                } else {
                    format!("- {} is referencing an invalid team - '{}'", path.to_string_lossy(), name)
                };
                if let Some(inherits_from) = inherits_from {
                    message.push_str(&format!(
                        "; this directory is currently inheriting its owner from {}",
                        inherits_from.to_string_lossy()
                    ));
                }
                vec![message]
            }
        }
    }
}

impl Errors {
    /// Supplementary detail that explains *what* is wrong without itself being an error.
    /// The stale-CODEOWNERS diff is surfaced here, as informational output, rather than
    /// inline with the error so that a long diff doesn't bury the actionable headline in
    /// CI logs (and so the wrapping `code_ownership` gem raises only the headline rather
    /// than the entire diff).
    pub fn info_messages(&self) -> Vec<String> {
        self.0
            .iter()
            .filter_map(|error| match error {
                Error::CodeownershipFileIsStale { diff, .. } if !diff.is_empty() => {
                    Some(format!("The following changes are required (- current, + expected):\n{diff}"))
                }
                _ => None,
            })
            .collect()
    }
}

impl Display for Errors {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let grouped_errors = self.0.iter().into_group_map_by(|error| error.category());
        let grouped_errors = Vec::from_iter(grouped_errors.iter());
        let grouped_errors = grouped_errors.iter().sorted_by_key(|(category, _)| category);

        for (category, errors) in grouped_errors {
            write!(f, "\n{}", category)?;

            let messages = errors.iter().flat_map(|error| error.messages()).sorted().join("\n");
            if !messages.is_empty() {
                writeln!(f)?;
                write!(f, "{}", messages)?;
            }

            writeln!(f)?;
        }

        Ok(())
    }
}

impl core::error::Error for Errors {}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;

    #[test]
    fn test_codeowners_diff_reports_added_and_removed_lines() {
        let current = indoc! {"
            # Team A
            /app/a.rb @TeamA
            /app/old.rb @TeamA
        "};
        let generated = indoc! {"
            # Team A
            /app/a.rb @TeamA
            /app/b.rb @TeamB
        "};

        let diff = codeowners_diff(current, generated);

        assert_eq!(diff, "-/app/old.rb @TeamA\n+/app/b.rb @TeamB");
    }

    #[test]
    fn test_codeowners_diff_against_empty_file_is_all_additions() {
        let generated = "# Team A\n/app/a.rb @TeamA\n";

        let diff = codeowners_diff("", generated);

        assert_eq!(diff, "+# Team A\n+/app/a.rb @TeamA");
    }

    #[test]
    fn test_codeowners_diff_is_empty_when_identical() {
        let file = "# Team A\n/app/a.rb @TeamA\n";

        assert_eq!(codeowners_diff(file, file), "");
    }
}
