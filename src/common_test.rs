#[cfg(test)]
pub mod tests {
    use std::{
        error::Error,
        fs::{self, File},
        path::PathBuf,
    };

    use indoc::indoc;

    use tempfile::tempdir;

    use crate::{
        cache::{noop::NoopCache, Cache},
        config::Config,
        ownership::Ownership,
        project_builder::ProjectBuilder,
    };

    macro_rules! ownership {
        ($($test_files:expr),+) => {{
            let temp_dir = tempdir()?;
            let test_config = TestConfig::new(
                temp_dir.path().to_path_buf(),
                vec![$($test_files),+]
            );
            build_ownership(test_config)
        }};
    }

    const DEFAULT_CODE_OWNERSHIP_YML: &str = indoc! {"
        ---
        owned_globs:
          - \"{app,components,config,frontend,lib,packs,spec}/**/*.{rb,rake,js,jsx,ts,tsx,json,yml}\"
        unowned_globs:
          - config/code_ownership.yml
        javascript_package_paths:
          - javascript/packages/**
        vendored_gems_path: gems
        team_file_glob:
          - config/teams/**/*.yml
    "};

    #[derive(Debug)]
    pub struct TestConfig {
        pub temp_dir_path: PathBuf,
        pub team_names: Vec<String>,
        pub files: Vec<TestProjectFile>,

        pub code_ownership_config_yml: String,
        pub relative_code_ownership_config_yml_path: String,
        pub relative_teams_path: String,
        pub generate_codeowners: bool,
    }

    impl Default for TestConfig {
        fn default() -> Self {
            Self {
                code_ownership_config_yml: DEFAULT_CODE_OWNERSHIP_YML.to_owned(),
                relative_code_ownership_config_yml_path: "config/code_ownership.yml".to_owned(),
                relative_teams_path: "config/teams".to_owned(),
                generate_codeowners: true,
                temp_dir_path: PathBuf::default(),
                team_names: vec!["Bar".to_owned(), "Foo".to_owned(), "Baz".to_owned(), "Bam".to_owned()],
                files: vec![],
            }
        }
    }

    impl TestConfig {
        pub fn new(temp_dir_path: PathBuf, files: Vec<TestProjectFile>) -> Self {
            Self {
                temp_dir_path,
                files,
                ..Default::default()
            }
        }
    }

    #[derive(Debug, Default)]
    pub struct TestProjectFile {
        pub relative_path: String,
        pub content: String,
    }

    pub fn build_ownership(test_config: TestConfig) -> Result<Ownership, Box<dyn Error>> {
        fs::create_dir_all(test_config.temp_dir_path.join(".github"))?;
        fs::create_dir_all(test_config.temp_dir_path.join(&test_config.relative_teams_path))?;

        fs::write(
            test_config.temp_dir_path.join(&test_config.relative_code_ownership_config_yml_path),
            test_config.code_ownership_config_yml,
        )?;

        let relative_teams_path = &test_config.relative_teams_path;
        for name in test_config.team_names.iter() {
            let team_yml = format!("name: {}\ngithub:\n  team: \"@{}\"\n  members:\n    - {}member\n", name, name, name);
            fs::write(
                test_config
                    .temp_dir_path
                    .join(relative_teams_path)
                    .join(format!("{}.yml", name.to_lowercase())),
                team_yml,
            )?;
        }

        for project_file in test_config.files.iter() {
            if let Some(parent_dir) = PathBuf::from(&project_file.relative_path).parent() {
                fs::create_dir_all(test_config.temp_dir_path.join(parent_dir))?;
            }
            fs::write(test_config.temp_dir_path.join(&project_file.relative_path), &project_file.content)?;
        }

        let config_file = File::open(test_config.temp_dir_path.join(test_config.relative_code_ownership_config_yml_path))?;
        let config: Config = serde_yaml::from_reader(config_file)?;

        let codeowners_file_path = &test_config.temp_dir_path.join(".github/CODEOWNERS");
        let cache: Cache = NoopCache::default().into();
        let mut builder = ProjectBuilder::new(
            &config,
            test_config.temp_dir_path.clone(),
            codeowners_file_path.clone(),
            false,
            &cache,
        );
        let project = builder.build()?;
        let ownership = Ownership::build(project);
        if test_config.generate_codeowners {
            std::fs::write(codeowners_file_path, ownership.generate_file())?;
        }
        // rebuild project to ensure new codeowners file is read
        let mut builder = ProjectBuilder::new(
            &config,
            test_config.temp_dir_path.clone(),
            codeowners_file_path.clone(),
            false,
            &cache,
        );
        let project = builder.build()?;
        Ok(Ownership::build(project))
    }

    pub fn build_ownership_with_directory_codeowners() -> Result<Ownership, Box<dyn Error>> {
        ownership!(
            TestProjectFile {
                relative_path: "app/consumers/deep/nesting/nestdir/deep_file.rb".to_owned(),
                content: "class DeepFile\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/consumers/one_owner.rb".to_owned(),
                content: "class OneOwner\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/services/service_file.rb".to_owned(),
                content: "class ServiceFile\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/services/some_other_file.rb".to_owned(),
                content: "class SomeOtherFile\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/consumers/.codeowner".to_owned(),
                content: "Bar\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/services/.codeowner".to_owned(),
                content: "Foo\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/services/exciting/.codeowner".to_owned(),
                content: "Bar\n".to_owned(),
            }
        )
    }

    pub fn build_ownership_with_directory_codeowners_with_brackets() -> Result<Ownership, Box<dyn Error>> {
        ownership!(
            TestProjectFile {
                relative_path: "app/[consumers]/deep/nesting/[nestdir]/deep_file.rb".to_owned(),
                content: "class DeepFile\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/[consumers]/one_owner.rb".to_owned(),
                content: "class OneOwner\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/services/service_file.rb".to_owned(),
                content: "class ServiceFile\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/services/some_other_file.rb".to_owned(),
                content: "class SomeOtherFile\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/[consumers]/.codeowner".to_owned(),
                content: "Bar\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/[consumers]/deep/nesting/[nestdir]/.codeowner".to_owned(),
                content: "Foo\n".to_owned(),
            }
        )
    }

    pub fn build_ownership_with_all_mappers() -> Result<Ownership, Box<dyn Error>> {
        ownership!(
            TestProjectFile {
                relative_path: "app/consumers/directory_owned.rb".to_owned(),
                content: "class DirectoryOwned\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "app/consumers/.codeowner".to_owned(),
                content: "Bar\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "packs/foo/package.yml".to_owned(),
                content: "owner: Baz\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "packs/foo/app/services/package_owned.rb".to_owned(),
                content: "class PackageOwned\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "packs/bar/app/services/team_file_owned.rb".to_owned(),
                content: "class GlobMapperOwned\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "config/teams/baz.yml".to_owned(),
                content: "name: Baz\ngithub:\n  team: \"@Baz\"\n  members:\n    - Baz member\nowned_globs:\n  - \"packs/bar/**\"\n"
                    .to_owned(),
            },
            TestProjectFile {
                relative_path: "packs/zebra/app/services/team_file_owned.rb".to_owned(),
                content: "# @team Foo\nclass TeamFileOwned\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "packs/jscomponents/comp.ts".to_owned(),
                content: "// @team Foo\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "gems/taco/sauce.rb".to_owned(),
                content: "class Taco::Sauce\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "config/teams/bam.yml".to_owned(),
                content: "name: Bam\ngithub:\n  team: \"@Bam\"\n  members:\n    - Bam member\nruby:\n  owned_gems:\n    - taco\n"
                    .to_owned(),
            }
        )
    }
    pub fn build_ownership_with_team_file_codeowners() -> Result<Ownership, Box<dyn Error>> {
        let temp_dir = tempdir()?;

        let test_config = TestConfig::new(
            temp_dir.path().to_path_buf(),
            vec![
                TestProjectFile {
                    relative_path: "packs/jscomponents/comp.ts".to_owned(),
                    content: "// @team Foo\n".to_owned(),
                },
                TestProjectFile {
                    relative_path: "packs/[admin]/comp.ts".to_owned(),
                    content: "// @team Bar\n".to_owned(),
                },
                TestProjectFile {
                    relative_path: "packs/bar/comp.rb".to_owned(),
                    content: "// @team Bar\n".to_owned(),
                },
            ],
        );
        build_ownership(test_config)
    }
    pub fn build_ownership_with_team_gem_codeowners() -> Result<Ownership, Box<dyn Error>> {
        ownership!(
            TestProjectFile {
                relative_path: "gems/globbing/globber.rb".to_owned(),
                content: "class Globbing::Globber\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "config/teams/bam.yml".to_owned(),
                content: "name: Bam\ngithub:\n  team: \"@Bam\"\n  members:\n    - Bam member\nruby:\n  owned_gems:\n    - globbing\n"
                    .to_owned(),
            }
        )
    }

    pub fn build_ownership_with_team_glob_codeowners() -> Result<Ownership, Box<dyn Error>> {
        ownership!(TestProjectFile {
            relative_path: "config/teams/baz.yml".to_owned(),
            content: "name: Baz\ngithub:\n  team: \"@Baz\"\n  members:\n    - Baz member\nowned_globs:\n  - \"packs/bar/**\"\n".to_owned(),
        })
    }

    pub fn build_ownership_with_team_yml_codeowners() -> Result<Ownership, Box<dyn Error>> {
        let temp_dir = tempdir()?;

        let test_config = TestConfig::new(temp_dir.path().to_path_buf(), vec![]);
        build_ownership(test_config)
    }
    pub fn build_ownership_with_package_codeowners() -> Result<Ownership, Box<dyn Error>> {
        ownership!(
            TestProjectFile {
                relative_path: "packs/foo/package.yml".to_owned(),
                content: "owner: Baz\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "packs/foo/app/services/package_owned.rb".to_owned(),
                content: "class PackageOwned\nend\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "packs/bam/package.yml".to_owned(),
                content: "owner: Bam\n".to_owned(),
            },
            TestProjectFile {
                relative_path: "packs/bam/app/services/package_owned.rb".to_owned(),
                content: "class PackageOwned\nend\n".to_owned(),
            }
        )
    }

    pub fn vecs_match<T: PartialEq + std::fmt::Debug>(a: &Vec<T>, b: &Vec<T>) {
        // First check lengths match
        assert_eq!(a.len(), b.len(), "Vectors have different lengths");

        // Check each element in a exists in b
        for elem_a in a {
            assert!(
                b.contains(elem_a),
                "Element {:?} from first vector not found in second vector",
                elem_a
            );
        }

        // Check each element in b exists in a
        for elem_b in b {
            assert!(
                a.contains(elem_b),
                "Element {:?} from second vector not found in first vector",
                elem_b
            );
        }
    }
}
