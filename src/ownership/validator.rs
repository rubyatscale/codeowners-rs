use crate::path_utils::matches_globs;
use crate::project::{Package, Project, ProjectFile, Team};
use core::fmt;
use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use itertools::Itertools;
use rayon::prelude::IntoParallelIterator;
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
    pub executable_name: String,
}

/// A path a caller asked to validate.
pub struct SuppliedPath {
    /// The path as given, which is how it's reported.
    pub given: String,
    /// What the matchers see: the project-relative path, or the given path, without the root if it's absolute, when it names
    /// nothing under the project.
    pub path: PathBuf,
    pub in_project: bool,
}

struct OwnershipQuery<'a> {
    shown_as: &'a Path,
    path: &'a Path,
    // optionally_owned_globs never excuses a path outside the project.
    in_project: bool,
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
    /// A supplied file whose one owner CODEOWNERS doesn't give it, the part of a stale CODEOWNERS a scoped run can see.
    FileOwnerNotInCodeownersFile {
        executable_name: String,
        path: PathBuf,
        owner: TeamName,
        listed: Option<TeamName>,
    },
}

#[derive(Debug)]
pub struct Errors(Vec<Error>);

impl Validator {
    #[instrument(name = "validator_validate", level = "debug", skip_all)]
    pub fn validate(&self, file_generator: &FileGenerator) -> Result<(), Errors> {
        let mut validation_errors = Vec::new();
        let files: Vec<(&Path, &ProjectFile)> = self
            .project
            .files
            .iter()
            .map(|file| (self.project.relative_path(&file.path), file))
            .collect();
        let packages: Vec<&Package> = self.project.packages.iter().collect();
        let queries: Vec<OwnershipQuery> = files
            .iter()
            .map(|(relative_path, _)| OwnershipQuery {
                shown_as: relative_path,
                path: relative_path,
                in_project: true,
            })
            .collect();

        debug!("validate_invalid_team");
        validation_errors.append(&mut self.validate_invalid_team(&files, &packages));
        validation_errors.append(&mut self.invalid_directory_ownership(None));

        debug!("validate_file_ownership");
        validation_errors.append(&mut self.validate_file_ownership(self.path_to_owners(&queries)));

        debug!("validate_codeowners_file");
        validation_errors.append(&mut self.validate_codeowners_file(file_generator));

        if validation_errors.is_empty() {
            Ok(())
        } else {
            Err(Errors(validation_errors))
        }
    }

    /// Runs the per-file checks of [`Validator::validate`] over just `supplied`, resolving ownership through the mappers as
    /// the whole-project run does. Annotations are read only from files the project walked, so an untracked file's own
    /// annotation doesn't count, as in `validate`.
    ///
    /// A package or `.codeowner` is checked if its directory contains one of `selectors`, which can include paths outside
    /// `owned_globs` such as the `package.yml` or `.codeowner` itself, so that one bad one elsewhere doesn't fail every scoped
    /// run. The staleness check compares the whole file and can't be scoped, so instead each supplied file with one owner is
    /// checked against `listed`, the team CODEOWNERS gives each path as given. Pass `None` when CODEOWNERS was just generated.
    #[instrument(name = "validator_validate_scoped", level = "debug", skip_all)]
    pub fn validate_files(
        &self,
        supplied: &[SuppliedPath],
        selectors: &[PathBuf],
        listed: Option<&HashMap<String, Option<Team>>>,
    ) -> Result<(), Errors> {
        let supplied: Vec<&SuppliedPath> = supplied.iter().unique_by(|supplied| &supplied.given).collect();
        let in_project: HashSet<&Path> = supplied
            .iter()
            .filter(|supplied| supplied.in_project)
            .map(|supplied| supplied.path.as_path())
            .collect();
        let walked: HashMap<&Path, &ProjectFile> = self
            .project
            .files
            .iter()
            .map(|file| (self.project.relative_path(&file.path), file))
            .filter(|(relative_path, _)| in_project.contains(relative_path))
            .collect();
        let files: Vec<(&Path, &ProjectFile)> = supplied
            .iter()
            .filter(|supplied| supplied.in_project)
            .filter_map(|supplied| walked.get(supplied.path.as_path()).map(|file| (Path::new(&supplied.given), *file)))
            .collect();
        let packages: Vec<&Package> = self
            .project
            .packages
            .iter()
            .filter(|package| self.package_contains_any(package, selectors))
            .collect();
        let queries: Vec<OwnershipQuery> = supplied
            .iter()
            .map(|supplied| OwnershipQuery {
                shown_as: Path::new(&supplied.given),
                path: &supplied.path,
                in_project: supplied.in_project,
            })
            .collect();

        let mut validation_errors = Vec::new();

        debug!("validate_invalid_team");
        validation_errors.append(&mut self.validate_invalid_team(&files, &packages));
        validation_errors.append(&mut self.invalid_directory_ownership(Some(selectors)));

        debug!("validate_file_ownership");
        let owners = self.path_to_owners(&queries);
        if let Some(listed) = listed {
            validation_errors.append(&mut self.owners_not_in_codeowners_file(&owners, listed));
        }
        validation_errors.append(&mut self.validate_file_ownership(owners));

        if validation_errors.is_empty() {
            Ok(())
        } else {
            Err(Errors(validation_errors))
        }
    }

    // A file with no owner or more than one is already reported, and so is a team that doesn't exist.
    fn owners_not_in_codeowners_file(
        &self,
        owners: &[(&OwnershipQuery, Vec<Owner>)],
        listed: &HashMap<String, Option<Team>>,
    ) -> Vec<Error> {
        owners
            .iter()
            .filter_map(|(query, owners)| {
                let [owner] = owners.as_slice() else {
                    return None;
                };
                let team = self.project.teams_by_name.get(&owner.team_name)?;
                let listed = listed
                    .get(query.shown_as.to_string_lossy().as_ref())
                    .and_then(|listed| listed.as_ref())
                    .map(|listed| listed.name.clone());
                (listed.as_ref() != Some(&team.name)).then(|| Error::FileOwnerNotInCodeownersFile {
                    executable_name: self.executable_name.clone(),
                    path: query.shown_as.to_path_buf(),
                    owner: team.name.clone(),
                    listed,
                })
            })
            .collect()
    }

    // `starts_with` compares whole components, so `packs/foo` doesn't contain `packs/foobar`. A package at the root has an
    // empty relative root, which contains every path.
    fn package_contains_any(&self, package: &Package, selectors: &[PathBuf]) -> bool {
        package
            .package_root()
            .map(|package_root| self.project.relative_path(package_root))
            .is_some_and(|package_root| selectors.iter().any(|path| path.starts_with(package_root)))
    }

    #[instrument(name = "validate_invalid_team", level = "debug", skip_all)]
    fn validate_invalid_team(&self, files: &[(&Path, &ProjectFile)], packages: &[&Package]) -> Vec<Error> {
        debug!("validating project");
        let mut errors: Vec<Error> = Vec::new();

        let team_names: HashSet<&TeamName> = self.project.teams.iter().map(|team| &team.name).collect();

        errors.append(&mut self.invalid_team_annotation(&team_names, files));
        errors.append(&mut self.invalid_package_ownership(&team_names, packages));

        errors
    }

    fn invalid_team_annotation(&self, team_names: &HashSet<&String>, files: &[(&Path, &ProjectFile)]) -> Vec<Error> {
        files
            .par_iter()
            .flat_map(|(shown_as, file)| {
                if let Some(owner) = &file.owner
                    && !team_names.contains(owner)
                {
                    return Some(Error::InvalidTeam {
                        name: owner.clone(),
                        path: shown_as.to_path_buf(),
                    });
                }

                None
            })
            .collect()
    }

    fn invalid_package_ownership(&self, team_names: &HashSet<&String>, packages: &[&Package]) -> Vec<Error> {
        packages
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
    fn invalid_directory_ownership(&self, selectors: Option<&[PathBuf]>) -> Vec<Error> {
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
            .filter(|directory_codeowner_file| {
                selectors.is_none_or(|selectors| {
                    directory_codeowner_file
                        .directory_root()
                        .map(|root| self.project.relative_path(root))
                        .is_some_and(|root| selectors.iter().any(|path| path.starts_with(root)))
                })
            })
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
    fn validate_file_ownership(&self, owners: Vec<(&OwnershipQuery, Vec<Owner>)>) -> Vec<Error> {
        let optionally_owned_globs = &self.project.optionally_owned_globs;

        owners
            .into_par_iter()
            .filter_map(|(query, owners)| {
                if owners.is_empty() {
                    if query.in_project && matches_globs(query.path, optionally_owned_globs) {
                        None
                    } else {
                        Some(Error::FileWithoutOwner {
                            path: query.shown_as.to_path_buf(),
                        })
                    }
                } else if owners.len() > 1 {
                    Some(Error::FileWithMultipleOwners {
                        path: query.shown_as.to_path_buf(),
                        owners,
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    #[instrument(name = "validate_codeowners_file", level = "debug", skip_all)]
    fn validate_codeowners_file(&self, file_generator: &FileGenerator) -> Vec<Error> {
        let generated_file = file_generator.generate_file();
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

    // Keyed on paths rather than ProjectFiles so that a scoped run can ask about a path the walk didn't record, such as an
    // untracked file in a directory with a `.codeowner`, and agree with `for-file`.
    #[instrument(name = "path_to_owners", level = "debug", skip_all)]
    fn path_to_owners<'a>(&self, queries: &'a [OwnershipQuery<'a>]) -> Vec<(&'a OwnershipQuery<'a>, Vec<Owner>)> {
        let owner_matchers: Vec<OwnerMatcher> = self.mappers.iter().flat_map(|mapper| mapper.owner_matchers()).collect();
        let file_owner_finder = FileOwnerFinder {
            owner_matchers: &owner_matchers,
        };

        queries
            .par_iter()
            .map(|query| (query, file_owner_finder.find(query.path)))
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
                Error::CodeownershipFileIsStale { executable_name, diff: _ }
                | Error::FileOwnerNotInCodeownersFile { executable_name, .. } => {
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
            Error::FileOwnerNotInCodeownersFile { path, owner, listed, .. } => vec![match listed {
                Some(listed) => format!(
                    "- {} is owned by {}, but CODEOWNERS gives it to {}",
                    path.to_string_lossy(),
                    owner,
                    listed
                ),
                // CODEOWNERS may have no line for the file, or a line naming a team that's no longer in the team files.
                None => format!(
                    "- {} is owned by {}, but CODEOWNERS doesn't give it to a known team",
                    path.to_string_lossy(),
                    owner
                ),
            }],
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
                    let inherits_from = if inherits_from.as_os_str().is_empty() {
                        "the project root".into()
                    } else {
                        inherits_from.to_string_lossy()
                    };
                    message.push_str(&format!("; this directory is currently inheriting its owner from {inherits_from}"));
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
    use crate::project::{DirectoryCodeownersFile, PackageType, Team};
    use indoc::indoc;

    const ROOT: &str = "/proj";

    // No mappers means no file has an owner, so every file in scope is reported as unowned, which makes the scoping visible.
    fn validator(files: &[&str], packages: &[(&str, &str)], codeowner_files: &[(&str, &str)], teams: &[&str]) -> Validator {
        let teams: Vec<Team> = teams
            .iter()
            .map(|name| Team {
                name: (*name).to_string(),
                ..Default::default()
            })
            .collect();
        let project = Project {
            base_path: PathBuf::from(ROOT),
            files: files
                .iter()
                .map(|path| ProjectFile {
                    owner: None,
                    path: PathBuf::from(ROOT).join(path),
                })
                .collect(),
            packages: packages
                .iter()
                .map(|(path, owner)| Package {
                    path: PathBuf::from(ROOT).join(path),
                    package_type: PackageType::Ruby,
                    owner: (*owner).to_string(),
                })
                .collect(),
            vendored_gems: vec![],
            teams_by_name: teams.iter().map(|team| (team.name.clone(), team.clone())).collect(),
            teams,
            codeowners_file_path: PathBuf::from(".github/CODEOWNERS"),
            directory_codeowner_files: codeowner_files
                .iter()
                .map(|(path, owner)| DirectoryCodeownersFile {
                    path: PathBuf::from(ROOT).join(path),
                    owner: (*owner).to_string(),
                })
                .collect(),
            executable_name: "codeowners".to_string(),
            optionally_owned_globs: vec![],
        };

        Validator {
            project: Arc::new(project),
            mappers: vec![],
            executable_name: "codeowners".to_string(),
        }
    }

    fn supplied(paths: &[&str]) -> Vec<SuppliedPath> {
        paths
            .iter()
            .map(|path| SuppliedPath {
                given: (*path).to_string(),
                path: PathBuf::from(path),
                in_project: true,
            })
            .collect()
    }

    fn selectors(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    fn report(result: Result<(), Errors>) -> String {
        result.map_or_else(|errors| errors.to_string(), |_| String::new())
    }

    #[test]
    fn validate_files_reports_only_the_supplied_files() {
        let validator = validator(&["ruby/a.rb", "ruby/b.rb"], &[], &[], &[]);

        let report = report(validator.validate_files(&supplied(&["ruby/a.rb"]), &selectors(&["ruby/a.rb"]), None));

        assert!(report.contains("ruby/a.rb"), "{report}");
        assert!(!report.contains("ruby/b.rb"), "{report}");
    }

    #[test]
    fn validate_files_reports_a_path_supplied_twice_once() {
        let validator = validator(&[], &[], &[], &[]);

        let errors = validator
            .validate_files(&supplied(&["ruby/ghost.rb", "ruby/ghost.rb"]), &selectors(&["ruby/ghost.rb"]), None)
            .expect_err("an unowned path is an error");

        assert_eq!(errors.0.len(), 1, "{errors:?}");
    }

    #[test]
    fn validate_files_skips_a_package_or_codeowner_containing_no_supplied_path() {
        let validator = validator(
            &[],
            &[("ruby/packages/foo/package.yml", "NoSuchTeam")],
            &[("ruby/legacy/.codeowner", "NoSuchTeam")],
            &["Payroll"],
        );

        assert_eq!(report(validator.validate_files(&[], &selectors(&["ruby/app/a.rb"]), None)), "");
    }

    #[test]
    fn validate_files_reports_a_package_or_codeowner_containing_a_supplied_path() {
        let validator = validator(
            &[],
            &[("ruby/packages/foo/package.yml", "NoSuchTeam")],
            &[("ruby/packages/foo/app/.codeowner", "NoSuchTeam")],
            &["Payroll"],
        );

        let report = report(validator.validate_files(&[], &selectors(&["ruby/packages/foo/app/a.rb"]), None));

        assert!(report.contains("ruby/packages/foo/package.yml"), "{report}");
        assert!(report.contains("ruby/packages/foo/app/.codeowner"), "{report}");
    }

    #[test]
    fn validate_files_reports_a_supplied_package_or_codeowner_itself() {
        // Neither matches owned_globs, so each reaches the check only as a selector, and neither is reported as unowned.
        let validator = validator(
            &[],
            &[("ruby/packages/foo/package.yml", "NoSuchTeam")],
            &[("ruby/legacy/.codeowner", "NoSuchTeam")],
            &["Payroll"],
        );

        let report = report(validator.validate_files(&[], &selectors(&["ruby/packages/foo/package.yml", "ruby/legacy/.codeowner"]), None));

        assert!(report.contains("ruby/packages/foo/package.yml"), "{report}");
        assert!(report.contains("ruby/legacy/.codeowner"), "{report}");
        assert!(!report.contains("missing ownership"), "{report}");
    }

    #[test]
    fn validate_files_does_not_select_a_sibling_by_name_prefix() {
        let validator = validator(
            &[],
            &[("ruby/packages/foo/package.yml", "NoSuchTeam")],
            &[("ruby/legacy/.codeowner", "NoSuchTeam")],
            &["Payroll"],
        );

        assert_eq!(
            report(validator.validate_files(&[], &selectors(&["ruby/packages/foobar/a.rb", "ruby/legacy2/a.rb"]), None)),
            ""
        );
    }

    #[test]
    fn validate_files_selects_a_package_or_codeowner_at_the_project_root() {
        let validator = validator(&[], &[("package.yml", "NoSuchTeam")], &[(".codeowner", "NoSuchTeam")], &["Payroll"]);

        let report = report(validator.validate_files(&[], &selectors(&["ruby/app/anything.rb"]), None));

        assert!(report.contains("- package.yml"), "{report}");
        assert!(report.contains("- .codeowner"), "{report}");
    }

    #[test]
    fn validate_files_accepts_a_valid_package_or_codeowner() {
        let validator = validator(
            &[],
            &[("ruby/packages/foo/package.yml", "Payroll")],
            &[("ruby/packages/foo/.codeowner", "Payroll")],
            &["Payroll"],
        );

        assert_eq!(
            report(validator.validate_files(&[], &selectors(&["ruby/packages/foo/a.rb"]), None)),
            ""
        );
    }

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
