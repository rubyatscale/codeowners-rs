use ownership::{Ownership, ValidationErrors};
use tracing::debug;

use crate::project::Project;
use clap::{Parser, Subcommand};
use path_clean::PathClean;
use std::{
    error::Error,
    fs::File,
    path::{Path, PathBuf},
    process,
};

mod config;
mod ownership;
mod project;

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate the CODEOWNERS file and save it to '--codeowners-file-path'.
    Generate,

    /// Validate the validity of the CODEOWNERS file. A validation failure will exit with a failure code and a detailed output of the validation errors.
    Validate,

    /// Chains both 'generate' and 'verify' commands
    GenerateAndValidate,
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
    #[arg(long, default_value = "./config/codeowners-rs.yml")]
    config_path: PathBuf,

    /// Path for the root of the project
    #[arg(long, default_value = ".")]
    project_root: PathBuf,
}

impl Args {
    fn absolute_project_root(&self) -> Result<PathBuf, std::io::Error> {
        self.project_root.canonicalize()
    }

    fn absolute_config_path(&self) -> Result<PathBuf, std::io::Error> {
        Ok(self.absolute_path(&self.config_path)?.clean())
    }

    fn absolute_codeowners_path(&self) -> Result<PathBuf, std::io::Error> {
        Ok(self.absolute_path(&self.codeowners_file_path)?.clean())
    }

    fn absolute_path(&self, path: &Path) -> Result<PathBuf, std::io::Error> {
        Ok(self.absolute_project_root()?.join(path))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    install_logger();
    print_validation_errors_to_stdout(cli())?;

    Ok(())
}

fn cli() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let config_path = args.absolute_config_path()?;
    let codeowners_file_path = args.absolute_codeowners_path()?;
    let project_root = args.absolute_project_root()?;

    debug!(
        config_path = &config_path.to_str(),
        codeowners_file_path = &codeowners_file_path.to_str(),
        project_root = &project_root.to_str(),
    );

    let config = serde_yaml::from_reader(File::open(config_path)?)?;
    let ownership = Ownership::build(Project::build(&project_root, &codeowners_file_path, &config)?);
    let command = args.command;

    match command {
        Command::Validate => ownership.validate()?,
        Command::Generate => {
            std::fs::write(codeowners_file_path, ownership.generate_file())?;
        }
        Command::GenerateAndValidate => {
            std::fs::write(codeowners_file_path, ownership.generate_file())?;
            ownership.validate()?
        }
    }

    Ok(())
}

fn print_validation_errors_to_stdout(result: Result<(), Box<dyn Error>>) -> Result<(), Box<dyn Error>> {
    if let Err(error) = result {
        if let Some(validation_errors) = error.downcast_ref::<ValidationErrors>() {
            println!("{}", validation_errors);
            process::exit(-1);
        } else {
            Err(error)?
        }
    }

    Ok(())
}

fn install_logger() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(true)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_level(true)
        .with_writer(std::io::stderr)
        .init();
}
