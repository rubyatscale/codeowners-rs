use ownership::Ownership;

use crate::{config::Config, project::Project};
use clap::{Parser, Subcommand};
use std::{error::Error, fs::File, path::PathBuf};

mod config;
mod ownership;
mod project;

use std::fmt;

#[derive(Debug)]
struct ValidationError(String);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Error for ValidationError {
    fn description(&self) -> &str {
        &self.0
    }
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Verify the validity of the CODEOWNERS file. A validation failure will exit with a failure code and a detailed output of the validation errors.
    Verify,

    /// Generate the CODEOWNERS file and save it to '--codeowners-file-path'.
    Generate,

    /// Chains the 'verify' and 'generate' commands, 'generate' will only be invoked if 'verify' succeeds.
    VerifyAndGenerate,
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

fn main() -> Result<(), Box<dyn Error>> {
    install_logger();
    cli()?;
    Ok(())
}

fn cli() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let config: Config = if args.config_path.exists() {
        serde_yaml::from_reader(File::open("./.config.yml")?)?
    } else {
        serde_yaml::from_str("")?
    };

    let codeowners_file_path = args.codeowners_file_path;
    let project_root = args.project_root;

    let ownership = Ownership::build(Project::build(&project_root, &codeowners_file_path, &config)?);
    let validation_errors = ownership.validate();

    let command = args.command;

    match command {
        Command::Verify => {
            let validation_errors = ownership.validate();
            if validation_errors.is_empty() {
                Ok(())
            } else {
                Err(ValidationError(format!("{:?}", validation_errors)))?
            }
        }
        Command::Generate => {
            std::fs::write(codeowners_file_path, ownership.generate_file())?;
            Ok(())
        }
        Command::VerifyAndGenerate => {
            if validation_errors.is_empty() {
                std::fs::write(codeowners_file_path, ownership.generate_file())?;
                Ok(())
            } else {
                println!("{:?}", validation_errors);
                Err(ValidationError(format!("{:?}", validation_errors)))?
            }
        }
    }
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
