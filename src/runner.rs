use core::fmt;
use std::{fs::File, path::PathBuf};

use error_stack::{Context, Result, ResultExt};

use crate::{
    cache::{Cache, Caching, file::GlobalCache, noop::NoopCache},
    config::Config,
    ownership::{FileOwner, Ownership},
    project_builder::ProjectBuilder,
};

pub struct RunConfig {
    pub project_root: PathBuf,
    pub codeowners_file_path: PathBuf,
    pub config_path: PathBuf,
    pub no_cache: bool,
}

pub struct Runner {
    run_config: RunConfig,
    ownership: Ownership,
    cache: Cache,
}

#[derive(Debug)]
pub enum Error {
    Io,
    ValidationFailed,
}

impl Context for Error {}
impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io => fmt.write_str("Error::Io"),
            Error::ValidationFailed => fmt.write_str("Error::ValidationFailed"),
        }
    }
}

impl Runner {
    pub fn new(run_config: RunConfig) -> Result<Self, Error> {
        let config_file = File::open(&run_config.config_path)
            .change_context(Error::Io)
            .attach_printable(format!("Can't open config file: {}", &run_config.config_path.to_string_lossy()))?;

        let config: Config = serde_yaml::from_reader(config_file).change_context(Error::Io)?;
        let cache: Cache = if run_config.no_cache {
            NoopCache::default().into()
        } else {
            GlobalCache::new(run_config.project_root.clone(), config.cache_directory.clone())
                .change_context(Error::Io)?
                .into()
        };

        let mut project_builder = ProjectBuilder::new(
            &config,
            run_config.project_root.clone(),
            run_config.codeowners_file_path.clone(),
            &cache,
        );
        let project = project_builder.build().change_context(Error::Io)?;
        let ownership = Ownership::build(project);

        cache.persist_cache().change_context(Error::Io)?;

        Ok(Self {
            run_config,
            ownership,
            cache,
        })
    }

    pub fn validate(&self) -> Result<(), Error> {
        self.ownership.validate().change_context(Error::ValidationFailed)?;
        Ok(())
    }

    pub fn generate(&self) -> Result<(), Error> {
        std::fs::write(&self.run_config.codeowners_file_path, self.ownership.generate_file()).change_context(Error::Io)?;
        Ok(())
    }

    pub fn generate_and_validate(&self) -> Result<(), Error> {
        self.generate().change_context(Error::Io)?;
        self.validate().change_context(Error::ValidationFailed)?;
        Ok(())
    }

    pub fn for_file(&self, file_path: &str) -> Result<(), Error> {
        let file_owners = self.ownership.for_file(file_path).change_context(Error::Io)?;
        match file_owners.len() {
            0 => println!("{}", FileOwner::default()),
            1 => println!("{}", file_owners[0]),
            _ => {
                println!("Error: file is owned by multiple teams!");
                for file_owner in file_owners {
                    println!("\n{}", file_owner);
                }
            }
        }
        Ok(())
    }

    pub fn for_team(&self, team_name: &str) -> Result<(), Error> {
        match self.ownership.for_team(team_name) {
            Ok(team_ownerships) => {
                println!("# Code Ownership Report for `{}` Team", team_name);
                for team_ownership in team_ownerships {
                    println!("\n#{}", team_ownership.heading);
                    match team_ownership.globs.len() {
                        0 => println!("This team owns nothing in this category."),
                        _ => println!("{}", team_ownership.globs.join("\n")),
                    }
                }
            }
            Err(err) => println!("{}", err),
        }
        Ok(())
    }

    pub fn delete_cache(&self) -> Result<(), Error> {
        self.cache.delete_cache().change_context(Error::Io)?;
        Ok(())
    }
}
