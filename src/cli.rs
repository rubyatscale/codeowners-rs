use clap::{Parser, Subcommand};
use codeowners::{
    cache::{Cache, Caching, file::GlobalCache, noop::NoopCache},
    config::Config,
    ownership::{FileOwner, Ownership},
    project_builder::ProjectBuilder,
};
use core::fmt;
use error_stack::{Context, Result, ResultExt};
use path_clean::PathClean;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Subcommand, Debug)]
enum Command {
    #[clap(about = "Finds the owner of a given file.", visible_alias = "f")]
    ForFile { name: String },

    #[clap(about = "Finds code ownership information for a given team ", visible_alias = "t")]
    ForTeam { name: String },

    #[clap(
        about = "Generate the CODEOWNERS file and save it to '--codeowners-file-path'.",
        visible_alias = "g"
    )]
    Generate,

    #[clap(
        about = "Validate the validity of the CODEOWNERS file. A validation failure will exit with a failure code and a detailed output of the validation errors.",
        visible_alias = "v"
    )]
    Validate,

    #[clap(about = "Chains both `generate` and `validate` commands.", visible_alias = "gv")]
    GenerateAndValidate,

    #[clap(about = "Delete the cache file.", visible_alias = "d")]
    DeleteCache,
}

/// A CLI to validate and generate Github's CODEOWNERS file.
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Path for the CODEOWNERS file.
    #[arg(long, default_value = "./.github/CODEOWNERS")]
    codeowners_file_path: PathBuf,
    /// Path for the configuration file
    #[arg(long, default_value = "./config/code_ownership.yml")]
    config_path: PathBuf,

    /// Path for the root of the project
    #[arg(long, default_value = ".")]
    project_root: PathBuf,

    /// Run without the cache (good for CI, testing)
    #[arg(long)]
    no_cache: bool,
}

impl Args {
    fn absolute_project_root(&self) -> Result<PathBuf, Error> {
        self.project_root.canonicalize().change_context(Error::Io)
    }

    fn absolute_config_path(&self) -> Result<PathBuf, Error> {
        Ok(self.absolute_path(&self.config_path)?.clean())
    }

    fn absolute_codeowners_path(&self) -> Result<PathBuf, Error> {
        Ok(self.absolute_path(&self.codeowners_file_path)?.clean())
    }

    fn absolute_path(&self, path: &Path) -> Result<PathBuf, Error> {
        Ok(self.absolute_project_root()?.join(path))
    }
}

#[derive(Debug)]
pub enum Error {
    Io,
    ValidationFailed,
}

impl Context for Error {}

pub fn cli() -> Result<(), Error> {
    let args = Args::parse();

    let config_path = args.absolute_config_path()?;
    let codeowners_file_path = args.absolute_codeowners_path()?;
    let project_root = args.absolute_project_root()?;

    let config_file = File::open(&config_path)
        .change_context(Error::Io)
        .attach_printable(format!("Can't open config file: {}", config_path.to_string_lossy()))?;

    let config: Config = serde_yaml::from_reader(config_file).change_context(Error::Io)?;
    let cache: Cache = if args.no_cache {
        NoopCache::default().into()
    } else {
        GlobalCache::new(project_root.clone(), config.cache_directory.clone())
            .change_context(Error::Io)?
            .into()
    };

    let mut project_builder = ProjectBuilder::new(&config, project_root.clone(), codeowners_file_path.clone(), &cache);
    let project = project_builder.build().change_context(Error::Io)?;
    let ownership = Ownership::build(project);

    cache.persist_cache().change_context(Error::Io)?;

    match args.command {
        Command::Validate => ownership.validate().change_context(Error::ValidationFailed)?,
        Command::Generate => {
            std::fs::write(codeowners_file_path, ownership.generate_file()).change_context(Error::Io)?;
        }
        Command::GenerateAndValidate => {
            std::fs::write(codeowners_file_path, ownership.generate_file()).change_context(Error::Io)?;
            ownership.validate().change_context(Error::ValidationFailed)?
        }
        Command::ForFile { name } => {
            let file_owners = ownership.for_file(&name).change_context(Error::Io)?;
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
        }
        Command::ForTeam { name } => match ownership.for_team(&name) {
            Ok(team_ownerships) => {
                println!("# Code Ownership Report for `{}` Team", name);
                for team_ownership in team_ownerships {
                    println!("\n#{}", team_ownership.heading);
                    match team_ownership.globs.len() {
                        0 => println!("This team owns nothing in this category."),
                        _ => println!("{}", team_ownership.globs.join("\n")),
                    }
                }
            }
            Err(err) => println!("{}", err),
        },
        Command::DeleteCache => {
            cache.delete_cache().change_context(Error::Io)?;
        }
    }

    Ok(())
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io => fmt.write_str("Error::Io"),
            Error::ValidationFailed => fmt.write_str("Error::ValidationFailed"),
        }
    }
}
