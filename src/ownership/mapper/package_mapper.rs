use std::sync::Arc;

use super::Entry;
use super::{Mapper, OwnerMatcher};
use crate::project::{Package, PackageType, Project};
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
        let team_by_name = self.project.team_by_name();

        for package in self.project.packages.iter().filter(|package| &package.package_type == package_type) {
            let package_root = package.package_root().to_string_lossy();
            let team = team_by_name.get(&package.owner);

            if let Some(team) = team {
                entries.push(Entry {
                    path: format!("{}/**/**", package_root),
                    github_team: team.github_team.to_owned(),
                    team_name: team.name.to_owned(),
                    disabled: team.avoid_ownership,
                });
            }
        }

        entries
    }

    fn owner_matchers(&self, package_type: &PackageType) -> Vec<OwnerMatcher> {
        let mut owner_matchers: Vec<OwnerMatcher> = Vec::new();
        let team_by_name = self.project.team_by_name();

        let packages = &self.project.packages;
        let packages: Vec<&Package> = packages.iter().filter(|package| &package.package_type == package_type).collect();

        // Nested packs can create a duplicate ownership false positive.
        // We avoid it by treating nested packs as a single top level pack for the purpose of validations
        let packages = remove_nested_packages(&packages);

        for package in packages {
            let package_root = package.package_root().to_string_lossy();
            let team = team_by_name.get(&package.owner);

            if let Some(team) = team {
                owner_matchers.push(OwnerMatcher::Glob {
                    glob: format!("{}/**/**", package_root),
                    team_name: team.name.to_owned(),
                    source: format!("package_mapper ({:?} glob: {}/**/**)", &package_type, package_root),
                });
            }
        }

        owner_matchers
    }
}

fn remove_nested_packages<'a>(packages: &'a [&'a Package]) -> Vec<&'a Package> {
    let mut top_level_packages: Vec<&Package> = Vec::new();

    for package in packages.iter().sorted_by_key(|package| package.package_root()) {
        if let Some(last_package) = top_level_packages.last() {
            let last_package_root = last_package.package_root();
            if !package.package_root().starts_with(last_package_root) {
                top_level_packages.push(package);
            }
        } else {
            top_level_packages.push(package);
        }
    }

    top_level_packages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common_test::tests::{build_ownership_with_all_mappers, build_ownership_with_package_codeowners},
        ownership::mapper::RubyPackageMapper,
        project::{Package, PackageType},
    };
    use itertools::Itertools;
    use std::{error::Error, path::Path};
    #[test]
    fn test_remove_nested_packages() {
        let packages = vec![
            Package {
                path: Path::new("packs/a/package.yml").to_owned(),
                package_type: PackageType::Ruby,
                owner: "owner_a".to_owned(),
            },
            Package {
                path: Path::new("packs/a/b/e/package.yml").to_owned(),
                package_type: PackageType::Ruby,
                owner: "owner_b".to_owned(),
            },
            Package {
                path: Path::new("packs/a/b/c/e/d/f/package.yml").to_owned(),
                package_type: PackageType::Ruby,
                owner: "owner_b".to_owned(),
            },
            Package {
                path: Path::new("packs/c/package.yml").to_owned(),
                package_type: PackageType::Ruby,
                owner: "owner_a".to_owned(),
            },
        ];

        let packages = packages.iter().collect_vec();

        let package_paths = super::remove_nested_packages(&packages)
            .iter()
            .map(|package| package.path.to_str().unwrap())
            .collect_vec();

        assert_eq!(package_paths, vec!["packs/a/package.yml", "packs/c/package.yml"]);
    }

    #[test]
    fn test_entries() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_all_mappers()?;
        let mapper = RubyPackageMapper::build(ownership.project.clone());
        let entries = mapper.entries();
        assert_eq!(
            entries,
            vec![Entry {
                path: "packs/foo/**/**".to_owned(),
                github_team: "@Baz".to_owned(),
                team_name: "Baz".to_owned(),
                disabled: false
            }]
        );
        Ok(())
    }

    #[test]
    fn test_owner_matchers() -> Result<(), Box<dyn Error>> {
        let ownership = build_ownership_with_package_codeowners()?;
        let mapper = PackageMapper::build(ownership.project.clone());
        let mut owner_matchers = mapper.owner_matchers(&PackageType::Ruby);
        owner_matchers.sort_by_key(|e| match e {
            OwnerMatcher::Glob { glob, .. } => glob.clone(),
            OwnerMatcher::ExactMatches(_, source) => source.clone(),
        });
        let expected_owner_matchers = vec![
            OwnerMatcher::Glob {
                glob: "packs/bam/**/**".to_owned(),
                team_name: "Bam".to_owned(),
                source: "package_mapper (Ruby glob: packs/bam/**/**)".to_owned(),
            },
            OwnerMatcher::Glob {
                glob: "packs/foo/**/**".to_owned(),
                team_name: "Baz".to_owned(),
                source: "package_mapper (Ruby glob: packs/foo/**/**)".to_owned(),
            },
        ];
        assert_eq!(owner_matchers, expected_owner_matchers);
        Ok(())
    }
}
