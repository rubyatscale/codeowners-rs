use crate::project::{Package, Project, ProjectFile};
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
    pub executable_name: String,
}

#[derive(Debug)]
enum Error {
    InvalidTeam { name: String, path: PathBuf },
    FileWithoutOwner { path: PathBuf },
    FileWithMultipleOwners { path: PathBuf, owners: Vec<Owner> },
    CodeownershipFileIsStale { executable_name: String, diff: String },
}

#[derive(Debug)]
pub struct Errors(Vec<Error>);

impl Validator {
    /// Whole-project validation.
    ///
    /// The `FileGenerator` is a parameter rather than a field so that
    /// [`Validator::validate_files`], which cannot check staleness, is structurally
    /// incapable of being handed one it would never use.
    #[instrument(name = "validator_validate", level = "debug", skip_all)]
    pub fn validate(&self, file_generator: &FileGenerator) -> Result<(), Errors> {
        let mut validation_errors = Vec::new();
        let files: Vec<&ProjectFile> = self.project.files.iter().collect();
        let packages: Vec<&Package> = self.project.packages.iter().collect();
        let relative_paths: Vec<&Path> = files.iter().map(|file| self.project.relative_path(&file.path)).collect();

        debug!("validate_invalid_team");
        validation_errors.append(&mut self.validate_invalid_team(&files, &packages));

        debug!("validate_file_ownership");
        validation_errors.append(&mut self.validate_file_ownership(&relative_paths));

        debug!("validate_codeowners_file");
        validation_errors.append(&mut self.validate_codeowners_file(file_generator));

        if validation_errors.is_empty() {
            Ok(())
        } else {
            Err(Errors(validation_errors))
        }
    }

    /// Validation restricted to the supplied paths.
    ///
    /// Runs the same per-file checks as [`Validator::validate`] — invalid team
    /// annotations and file ownership — over just the named files. Ownership is resolved
    /// through the mappers, exactly as the whole-project run does, so a file owned two
    /// ways is reported rather than silently resolving to whichever owner happened to
    /// win in the generated CODEOWNERS.
    ///
    /// This scopes the *per-file* work, not all of it. Building the owner matchers is
    /// still O(repo): `TeamFileMapper::owner_matchers` enumerates every annotated file
    /// in the project. So the cost is a fixed O(repo) term plus a variable
    /// O(supplied paths × matchers) term, where the whole-project run pays
    /// O(repo × matchers) for the latter.
    ///
    /// Measured on a large monorepo (~130k files, ~18k-line CODEOWNERS; `codeowners-perf`,
    /// best of 3 warm): the variable term is what collapses — validation drops from 933ms
    /// whole-project to 28ms for one path and 56ms for 2000, so it is near-flat in the
    /// number of paths. Wall clock only improves 3.0s to 2.0s, because the ~1.9s project
    /// build is the fixed term and is paid either way. Scoping is worth about a second on
    /// a repo that size, not an order of magnitude.
    ///
    /// The staleness check is deliberately absent: it compares the entire generated
    /// file against the entire on-disk one and cannot be scoped. `generate_and_validate`
    /// makes it moot by regenerating first; a caller that needs it on its own must run
    /// [`Validator::validate`].
    ///
    /// Package ownership is scoped too, to packages containing at least one supplied
    /// path. Checking every package would mean validating one file can fail over a
    /// package that file has nothing to do with — and since the gem's `--diff` mode feeds
    /// a changeset in, a single pre-existing bad package owner would block every commit in
    /// the repo until it was fixed.
    ///
    /// Hence the two path lists. `owned_paths` are the paths the project walk would have
    /// considered, and are what the per-file checks run over. `supplied_paths` is
    /// everything the caller named, including paths the walk skips — which is what puts a
    /// package in scope, so that editing a `package.yml` into naming a nonexistent team is
    /// caught by the commit that does it, not merely by a later commit that happens to
    /// touch a file inside that package.
    ///
    /// The per-check spans (`validate_invalid_team`, `validate_file_ownership`) are
    /// shared with the whole-project run, so a profile tells the two apart by parent
    /// span — `validator_validate_scoped` here, `validator_validate` there — not by the
    /// child span name.
    #[instrument(name = "validator_validate_scoped", level = "debug", skip_all)]
    pub fn validate_files(&self, owned_paths: &[PathBuf], supplied_paths: &[PathBuf]) -> Result<(), Errors> {
        let requested: HashSet<&Path> = owned_paths.iter().map(PathBuf::as_path).collect();

        let files: Vec<&ProjectFile> = self
            .project
            .files
            .iter()
            .filter(|file| requested.contains(self.project.relative_path(&file.path)))
            .collect();

        let packages: Vec<&Package> = self
            .project
            .packages
            .iter()
            .filter(|package| self.package_contains_any(package, supplied_paths))
            .collect();

        let mut validation_errors = Vec::new();

        // Every requested path goes to the matchers, including ones the walk never
        // recorded. An untracked file is the case that matters: it is absent from
        // `project.files`, but the matchers can still attribute it -- a new file in a
        // directory with a `.codeowner` is owned the moment it exists. Reporting such a
        // path as unowned instead put `validate` at odds with `for-file` on the same path.
        //
        // Iterating the deduped set also means a path supplied twice is one defect.
        let requested_paths: Vec<&Path> = requested.iter().copied().collect();

        debug!("validate_invalid_team");
        validation_errors.append(&mut self.validate_invalid_team(&files, &packages));

        debug!("validate_file_ownership");
        validation_errors.append(&mut self.validate_file_ownership(&requested_paths));

        if validation_errors.is_empty() {
            Ok(())
        } else {
            Err(Errors(validation_errors))
        }
    }

    /// Whether any of `supplied_paths` lies inside `package`.
    ///
    /// The manifest itself counts, since it sits at the package root and so is prefixed by
    /// it. A package at the project root has an empty relative root, which every path is
    /// prefixed by — correctly, since it owns the whole tree.
    fn package_contains_any(&self, package: &Package, supplied_paths: &[PathBuf]) -> bool {
        let Some(package_root) = package.package_root() else {
            return false;
        };
        let package_root = self.project.relative_path(package_root);

        supplied_paths.iter().any(|path| path.starts_with(package_root))
    }

    #[instrument(name = "validate_invalid_team", level = "debug", skip_all)]
    fn validate_invalid_team(&self, files: &[&ProjectFile], packages: &[&Package]) -> Vec<Error> {
        debug!("validating project");
        let mut errors: Vec<Error> = Vec::new();

        let team_names: HashSet<&TeamName> = self.project.teams.iter().map(|team| &team.name).collect();

        errors.append(&mut self.invalid_team_annotation(&team_names, files));
        errors.append(&mut self.invalid_package_ownership(&team_names, packages));

        errors
    }

    fn invalid_team_annotation(&self, team_names: &HashSet<&String>, files: &[&ProjectFile]) -> Vec<Error> {
        let project = self.project.clone();

        files
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

    #[instrument(name = "validate_file_ownership", level = "debug", skip_all)]
    fn validate_file_ownership(&self, relative_paths: &[&Path]) -> Vec<Error> {
        let mut validation_errors = Vec::new();

        for (relative_path, owners) in self.path_to_owners(relative_paths) {
            if owners.is_empty() {
                validation_errors.push(Error::FileWithoutOwner {
                    path: relative_path.to_owned(),
                })
            } else if owners.len() > 1 {
                validation_errors.push(Error::FileWithMultipleOwners {
                    path: relative_path.to_owned(),
                    owners,
                })
            }
        }

        validation_errors
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

    /// Resolve ownership for project-relative paths.
    ///
    /// Keyed on paths rather than `ProjectFile`s because that is all the matchers consume,
    /// and because it lets a scoped run ask about a path the walk never recorded — an
    /// untracked file, say. Answering those from the matchers is what makes
    /// `validate <path>` agree with `for-file <path>`; assuming they were unowned did not.
    #[instrument(name = "path_to_owners", level = "debug", skip_all)]
    fn path_to_owners<'a>(&self, relative_paths: &[&'a Path]) -> Vec<(&'a Path, Vec<Owner>)> {
        let owner_matchers: Vec<OwnerMatcher> = self.mappers.iter().flat_map(|mapper| mapper.owner_matchers()).collect();
        let file_owner_finder = FileOwnerFinder {
            owner_matchers: &owner_matchers,
        };

        relative_paths
            .par_iter()
            .map(|relative_path| (*relative_path, file_owner_finder.find(relative_path)))
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
                Error::InvalidTeam { name: _, path: _ } => "Found invalid team annotations".to_owned(),
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
    use crate::project::{PackageType, Team};
    use indoc::indoc;
    use std::collections::HashMap;

    const ROOT: &str = "/proj";

    /// A validator over a synthetic project with no mappers.
    ///
    /// No mappers means no file resolves to an owner, so every file that makes it into
    /// scope is reported as unowned. That is the point: it makes the *scoping* visible
    /// without any ownership rules to reason about. `validate_files` is otherwise covered
    /// only end-to-end through the binary, which cannot isolate the predicate.
    fn validator(files: &[&str], packages: &[(&str, &str)], teams: &[&str]) -> Validator {
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
            teams: teams
                .iter()
                .map(|name| Team {
                    name: (*name).to_string(),
                    ..Default::default()
                })
                .collect(),
            codeowners_file_path: PathBuf::from(".github/CODEOWNERS"),
            directory_codeowner_files: vec![],
            teams_by_name: HashMap::new(),
            executable_name: "codeowners".to_string(),
        };

        Validator {
            project: Arc::new(project),
            mappers: vec![],
            executable_name: "codeowners".to_string(),
        }
    }

    fn paths(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn validate_files_reports_only_the_supplied_files() {
        let validator = validator(&["ruby/a.rb", "ruby/b.rb"], &[], &[]);

        let errors = validator
            .validate_files(&paths(&["ruby/a.rb"]), &paths(&["ruby/a.rb"]))
            .expect_err("unowned file should be an error");
        let report = format!("{}", errors);

        assert!(report.contains("ruby/a.rb"), "{report}");
        assert!(!report.contains("ruby/b.rb"), "an unsupplied file leaked into scope: {report}");
    }

    #[test]
    fn validate_files_reports_a_path_supplied_twice_once() {
        let validator = validator(&[], &[], &[]);

        let errors = validator
            .validate_files(
                &paths(&["ruby/ghost.rb", "ruby/ghost.rb"]),
                &paths(&["ruby/ghost.rb", "ruby/ghost.rb"]),
            )
            .expect_err("an unwalked path should be an error");

        assert_eq!(errors.0.len(), 1, "{errors:?}");
    }

    #[test]
    fn validate_files_skips_a_package_containing_no_supplied_path() {
        // The blast-radius case: one bad package owner elsewhere in the repo must not fail
        // a run scoped to an unrelated file.
        let validator = validator(&["ruby/app/a.rb"], &[("ruby/packages/foo/package.yml", "NoSuchTeam")], &["Payroll"]);

        let errors = validator
            .validate_files(&paths(&["ruby/app/a.rb"]), &paths(&["ruby/app/a.rb"]))
            .expect_err("the unowned file is still an error");
        let report = format!("{}", errors);

        assert!(!report.contains("NoSuchTeam"), "unrelated package leaked into scope: {report}");
    }

    #[test]
    fn validate_files_reports_a_package_containing_a_supplied_path() {
        let validator = validator(
            &["ruby/packages/foo/app/a.rb"],
            &[("ruby/packages/foo/package.yml", "NoSuchTeam")],
            &["Payroll"],
        );

        let supplied = paths(&["ruby/packages/foo/app/a.rb"]);
        let errors = validator
            .validate_files(&supplied, &supplied)
            .expect_err("bad package owner is an error");
        let report = format!("{}", errors);

        assert!(report.contains("NoSuchTeam"), "{report}");
        assert!(report.contains("ruby/packages/foo/package.yml"), "{report}");
    }

    #[test]
    fn validate_files_reports_a_package_whose_manifest_is_itself_supplied() {
        // A manifest does not match owned_globs, so it never appears in `owned_paths` --
        // it reaches the package check through `supplied_paths` only. Without this, editing
        // a manifest to name a nonexistent team would not be caught by the commit doing it.
        let validator = validator(&[], &[("ruby/packages/foo/package.yml", "NoSuchTeam")], &["Payroll"]);

        let errors = validator
            .validate_files(&[], &paths(&["ruby/packages/foo/package.yml"]))
            .expect_err("bad package owner is an error");
        let report = format!("{}", errors);

        assert!(report.contains("NoSuchTeam"), "{report}");
        assert!(
            !report.contains("missing ownership"),
            "a manifest is not eligible to be reported unowned: {report}"
        );
    }

    #[test]
    fn validate_files_does_not_select_a_sibling_package_by_name_prefix() {
        // `starts_with` is component-wise, so `ruby/packages/foo` must not swallow
        // `ruby/packages/foobar`. A plain string prefix check would.
        let validator = validator(&[], &[("ruby/packages/foo/package.yml", "NoSuchTeam")], &["Payroll"]);

        assert!(
            validator.validate_files(&[], &paths(&["ruby/packages/foobar/app/a.rb"])).is_ok(),
            "a sibling package sharing a name prefix was selected"
        );
    }

    #[test]
    fn validate_files_selects_a_package_at_the_project_root() {
        // A root-level manifest has an empty relative package root, and every path is
        // prefixed by the empty path -- correctly, since it owns the whole tree. Asserted
        // because the scoping predicate silently depends on it.
        let validator = validator(&[], &[("package.yml", "NoSuchTeam")], &["Payroll"]);

        let errors = validator
            .validate_files(&[], &paths(&["ruby/app/anything.rb"]))
            .expect_err("a root-level package owns every path");

        assert!(format!("{}", errors).contains("NoSuchTeam"));
    }

    #[test]
    fn validate_files_accepts_a_valid_package_owner() {
        let validator = validator(&[], &[("ruby/packages/foo/package.yml", "Payroll")], &["Payroll"]);

        assert!(
            validator.validate_files(&[], &paths(&["ruby/packages/foo/package.yml"])).is_ok(),
            "a package owned by a real team is not an error"
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
