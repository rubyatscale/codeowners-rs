use std::path::Path;
use std::sync::Arc;

use super::escaper::directory_glob;
use super::{Entry, Source};
use super::{Mapper, OwnerMatcher};
use crate::project::{Package, PackageType, Project, Team};
use itertools::Itertools;

pub struct RubyPackageMapper {
    project: Arc<Project>,
}

pub struct JavascriptPackageMapper {
    project: Arc<Project>,
}

struct PackageMapper {
    project: Arc<Project>,
}

impl RubyPackageMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }
}

impl Mapper for RubyPackageMapper {
    fn entries(&self) -> Vec<Entry> {
        PackageMapper::build(self.project.clone()).entries(&PackageType::Ruby)
    }

    fn owner_matchers(&self) -> Vec<OwnerMatcher> {
        PackageMapper::build(self.project.clone()).owner_matchers(&PackageType::Ruby)
    }

    fn name(&self) -> String {
        "Owner metadata key in package.yml".to_owned()
    }
}

impl JavascriptPackageMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }
}

impl Mapper for JavascriptPackageMapper {
    fn entries(&self) -> Vec<Entry> {
        PackageMapper::build(self.project.clone()).entries(&PackageType::Javascript)
    }

    fn owner_matchers(&self) -> Vec<OwnerMatcher> {
        PackageMapper::build(self.project.clone()).owner_matchers(&PackageType::Javascript)
    }

    fn name(&self) -> String {
        "Owner metadata key in package.json".to_owned()
    }
}

impl PackageMapper {
    pub fn build(project: Arc<Project>) -> Self {
        Self { project }
    }
}

impl PackageMapper {
    fn entries(&self, package_type: &PackageType) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();
        let team_by_name = self.project.teams_by_name.clone();

        for package in self.project.packages.iter().filter(|package| &package.package_type == package_type) {
            if let Some(package_root) = package.package_root() {
                let package_root = package_root.to_string_lossy();
                let team = team_by_name.get(&package.owner);

                if let Some(team) = team {
                    entries.push(Entry {
                        path: directory_glob(&package_root),
                        github_team: team.github_team.to_owned(),
                        team_name: team.name.to_owned(),
                        disabled: team.avoid_ownership,
                    });
                }
            }
        }

        entries
    }

    // A matcher leaves out the packages of its type nested in it, so a file gets its nearest package, as in CODEOWNERS and
    // `for-file`. A package naming an unknown team gets no matcher, so its files fall through to the enclosing package.
    fn owner_matchers(&self, package_type: &PackageType) -> Vec<OwnerMatcher> {
        let team_by_name = &self.project.teams_by_name;

        // Sorting by root puts the packages nested in a package right after it.
        let packages: Vec<(&Path, &Package, &Team)> = self
            .project
            .packages
            .iter()
            .filter(|package| &package.package_type == package_type)
            .filter_map(|package| Some((package.package_root()?, package, team_by_name.get(&package.owner)?)))
            .sorted_by_key(|(package_root, _, _)| *package_root)
            .collect();

        packages
            .iter()
            .enumerate()
            .map(|(index, (package_root, package, team))| {
                let glob = directory_glob(&package_root.to_string_lossy());
                OwnerMatcher::Glob {
                    glob: glob.clone(),
                    subtracted_globs: nested_package_roots(package_root, &packages[index + 1..])
                        .into_iter()
                        .map(|nested_root| directory_glob(&nested_root.to_string_lossy()))
                        .collect(),
                    team_name: team.name.to_owned(),
                    source: Source::Package(package.path.to_string_lossy().to_string(), glob),
                }
            })
            .collect()
    }
}

// The roots of the packages directly inside `package_root`, from the packages that sort after it. Leaving out a directly
// nested package also leaves out the packages inside it.
fn nested_package_roots<'a>(package_root: &Path, later_packages: &[(&'a Path, &Package, &Team)]) -> Vec<&'a Path> {
    let mut nested_roots: Vec<&'a Path> = Vec::new();
    for &(root, _, _) in later_packages.iter().take_while(|(root, _, _)| root.starts_with(package_root)) {
        if nested_roots.last().is_none_or(|nested_root| !root.starts_with(nested_root)) {
            nested_roots.push(root);
        }
    }
    nested_roots
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common_test::tests::{
            build_ownership_with_all_mappers, build_ownership_with_nested_package_codeowners, build_ownership_with_package_codeowners,
            build_ownership_with_package_codeowners_at_the_project_root, build_ownership_with_package_codeowners_with_glob_metacharacters,
            vecs_match,
        },
        ownership::mapper::RubyPackageMapper,
        project::PackageType,
    };
    use std::error::Error;

    #[test]
    fn test_entries() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let mapper = RubyPackageMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.entries(),
            &vec![Entry {
                path: "packs/foo/**/**".to_owned(),
                github_team: "@Baz".to_owned(),
                team_name: "Baz".to_owned(),
                disabled: false,
            }],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_package_codeowners()?;
        let mapper = PackageMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.owner_matchers(&PackageType::Ruby),
            &vec![
                OwnerMatcher::new_glob(
                    "packs/bam/**/**".to_owned(),
                    "Bam".to_owned(),
                    Source::Package("packs/bam/package.yml".to_owned(), "packs/bam/**/**".to_owned()),
                ),
                OwnerMatcher::new_glob(
                    "packs/foo/**/**".to_owned(),
                    "Baz".to_owned(),
                    Source::Package("packs/foo/package.yml".to_owned(), "packs/foo/**/**".to_owned()),
                ),
            ],
        );
        Ok(())
    }

    #[test]
    fn test_entries_with_glob_metacharacters() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_package_codeowners_with_glob_metacharacters()?;
        let mapper = RubyPackageMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.entries(),
            &vec![
                Entry {
                    path: "packs/\\[ab\\]/**/**".to_owned(),
                    github_team: "@Foo".to_owned(),
                    team_name: "Foo".to_owned(),
                    disabled: false,
                },
                Entry {
                    path: "packs/\\{legacy\\}/**/**".to_owned(),
                    github_team: "@Bar".to_owned(),
                    team_name: "Bar".to_owned(),
                    disabled: false,
                },
            ],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers_with_glob_metacharacters() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_package_codeowners_with_glob_metacharacters()?;
        let mapper = PackageMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.owner_matchers(&PackageType::Ruby),
            &vec![
                OwnerMatcher::new_glob(
                    "packs/\\[ab\\]/**/**".to_owned(),
                    "Foo".to_owned(),
                    Source::Package("packs/[ab]/package.yml".to_owned(), "packs/\\[ab\\]/**/**".to_owned()),
                ),
                OwnerMatcher::new_glob(
                    "packs/\\{legacy\\}/**/**".to_owned(),
                    "Bar".to_owned(),
                    Source::Package("packs/{legacy}/package.yml".to_owned(), "packs/\\{legacy\\}/**/**".to_owned()),
                ),
            ],
        );
        Ok(())
    }

    #[test]
    fn test_entries_at_the_project_root() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_package_codeowners_at_the_project_root()?;
        let mapper = RubyPackageMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.entries(),
            &vec![
                Entry {
                    path: "**/**".to_owned(),
                    github_team: "@Foo".to_owned(),
                    team_name: "Foo".to_owned(),
                    disabled: false,
                },
                Entry {
                    path: "packs/inner/**/**".to_owned(),
                    github_team: "@Bar".to_owned(),
                    team_name: "Bar".to_owned(),
                    disabled: false,
                },
            ],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers_at_the_project_root() -> Result<(), Box<dyn Error>> {
        // The root package encloses the other, so it leaves that one out, as for any nested package.
        let ownership = build_ownership_with_package_codeowners_at_the_project_root()?;
        let mapper = PackageMapper::build(ownership.project.clone());
        vecs_match(
            &mapper.owner_matchers(&PackageType::Ruby),
            &vec![
                OwnerMatcher::Glob {
                    glob: "**/**".to_owned(),
                    subtracted_globs: vec!["packs/inner/**/**".to_owned()],
                    team_name: "Foo".to_owned(),
                    source: Source::Package("package.yml".to_owned(), "**/**".to_owned()),
                },
                OwnerMatcher::new_glob(
                    "packs/inner/**/**".to_owned(),
                    "Bar".to_owned(),
                    Source::Package("packs/inner/package.yml".to_owned(), "packs/inner/**/**".to_owned()),
                ),
            ],
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers_leave_out_the_packages_nested_in_them() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_nested_package_codeowners()?;
        let mapper = PackageMapper::build(ownership.project.clone());
        let matcher = |root: &str, subtracted_roots: &[&str], team: &str| OwnerMatcher::Glob {
            glob: format!("{root}/**/**"),
            subtracted_globs: subtracted_roots.iter().map(|root| format!("{root}/**/**")).collect(),
            team_name: team.to_owned(),
            source: Source::Package(format!("{root}/package.yml"), format!("{root}/**/**")),
        };
        // `packs/foo/unknown` names no team, so it has no matcher and `packs/foo` keeps its files.
        vecs_match(
            &mapper.owner_matchers(&PackageType::Ruby),
            &vec![
                matcher("packs/foo", &["packs/foo/bam", "packs/foo/bar"], "Foo"),
                matcher("packs/foo/bam", &[], "Bam"),
                matcher("packs/foo/bar", &["packs/foo/bar/baz"], "Bar"),
                matcher("packs/foo/bar/baz", &[], "Baz"),
                matcher("packs/foobar", &[], "Bar"),
            ],
        );
        Ok(())
    }

    #[test]
    fn test_a_file_in_nested_packages_belongs_to_the_nearest_one() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_nested_package_codeowners()?;
        for (file, team) in [
            ("packs/foo/app/a.rb", "Foo"),
            ("packs/foo/bar/app/a.rb", "Bar"),
            ("packs/foo/bar/baz/app/a.rb", "Baz"),
            ("packs/foo/unknown/app/a.rb", "Foo"),
            ("packs/foobar/app/a.rb", "Bar"),
        ] {
            let owners = ownership.for_file(Path::new(file))?;
            let teams: Vec<&str> = owners.iter().map(|owner| owner.team.name.as_str()).collect();
            assert_eq!(teams, vec![team], "{file}");
        }
        Ok(())
    }
}
