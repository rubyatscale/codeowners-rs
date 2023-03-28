use ownership::{Ownership, ValidationErrors};

use crate::{config::Config, project::Project};
use clap::{Parser, Subcommand};
use std::{error::Error, fs::File, path::PathBuf, process};

mod config;
mod ownership;
mod project;

#[derive(Subcommand, Debug)]
enum Command {
    /// Generate the CODEOWNERS file and save it to '--codeowners-file-path'.
    Generate,

    /// Verify the validity of the CODEOWNERS file. A validation failure will exit with a failure code and a detailed output of the validation errors.
    Verify,

    /// Chains both 'generate' and 'verify' commands
    GenerateAndVerify,
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
    print_validation_errors_to_stdout(cli())?;

    Ok(())
}

fn cli() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let config: Config = if args.config_path.exists() {
        serde_yaml::from_reader(File::open(args.config_path)?)?
    } else {
        serde_yaml::from_str("")?
    };

    let codeowners_file_path = args.codeowners_file_path;
    let project_root = args.project_root;

    let ownership = Ownership::build(Project::build(&project_root, &codeowners_file_path, &config)?);
    let command = args.command;

    match command {
        Command::Verify => ownership.validate()?,
        Command::Generate => {
            std::fs::write(codeowners_file_path, ownership.generate_file())?;
        }
        Command::GenerateAndVerify => {
            std::fs::write(codeowners_file_path, ownership.generate_file())?;
            ownership.validate()?
        }
    }

    Ok(())
}

fn print_validation_errors_to_stdout(result: Result<(), Box<dyn Error>>) -> Result<(), Box<dyn Error>> {
    if let Err(error) = result {
        if let Some(validation_errors) = error.downcast_ref::<ValidationErrors>() {
            println!("{}", validation_errors.to_string());
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
