use std::rc::Rc;

use super::Entry;
use super::{Mapper, OwnerMatcher};
use crate::project::{Package, PackageType, Project};
use itertools::Itertools;

pub struct RubyPackageMapper {
    project: Rc<Project>,
}

pub struct JavascriptPackageMapper {
    project: Rc<Project>,
}

struct PackageMapper {
    project: Rc<Project>,
}

impl RubyPackageMapper {
    pub fn build(project: Rc<Project>) -> Self {
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
    pub fn build(project: Rc<Project>) -> Self {
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
    pub fn build(project: Rc<Project>) -> Self {
        Self { project }
    }
}

impl PackageMapper {
    fn entries(&self, package_type: &PackageType) -> Vec<Entry> {
        let mut entries: Vec<Entry> = Vec::new();
        let team_by_name = self.project.team_by_name();

        for package in self.project.packages.iter().filter(|package| &package.package_type == package_type) {
            let package_root = package.package_root().to_string_lossy();
            let team = team_by_name
                .get(&package.owner)
                .unwrap_or_else(|| panic!("Couldn't find team {}", package.owner));

            if team.avoid_ownership {
                continue;
            }

            entries.push(Entry {
                path: format!("{}/**/**", package_root),
                github_team: team.github_team.to_owned(),
                team_name: team.name.to_owned(),
            });
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

            let team = team_by_name
                .get(&package.owner)
                .unwrap_or_else(|| panic!("Couldn't find team {}", package.owner));

            owner_matchers.push(OwnerMatcher::Glob {
                glob: format!("{}/**/**", package_root),
                team_name: team.name.to_owned(),
                source: format!("package_mapper ({:?} glob: {}/**/**)", &package_type, package_root),
            });
        }

        owner_matchers
    }
}

fn remove_nested_packages<'a>(packages: &'a [&'a Package]) -> Vec<&'a Package> {
    packages
        .iter()
        .filter(|package| {
            let package_root = package.package_root();
            !packages.iter().any(|another_package| {
                let another_package_root = another_package.package_root();
                if package_root == another_package_root {
                    false
                } else {
                    package_root.starts_with(another_package_root)
                }
            })
        })
        .copied()
        .collect_vec()
}

#[cfg(test)]
mod tests {
    use crate::project::{Package, PackageType};
    use itertools::Itertools;
    use std::path::Path;

    #[test]
    fn test_remove_nested_packages() {
        let packages = vec![
            Package {
                path: Path::new("packs/a/package.yml").to_owned(),
                package_type: PackageType::Ruby,
                owner: "owner_a".to_owned(),
            },
            Package {
                path: Path::new("packs/a/b/package.yml").to_owned(),
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
}
