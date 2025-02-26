use clap::{Parser, Subcommand};
use codeowners::runner::{Error as RunnerError, RunResult};
use codeowners::runner::{RunConfig, Runner};
use error_stack::{Result, ResultExt};
use path_clean::PathClean;
use std::path::{Path, PathBuf};

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
    fn absolute_project_root(&self) -> Result<PathBuf, RunnerError> {
        self.project_root.canonicalize().change_context(RunnerError::Io(format!(
            "Can't canonicalize project root: {}",
            &self.project_root.to_string_lossy()
        )))
    }

    fn absolute_config_path(&self) -> Result<PathBuf, RunnerError> {
        Ok(self.absolute_path(&self.config_path)?.clean())
    }

    fn absolute_codeowners_path(&self) -> Result<PathBuf, RunnerError> {
        Ok(self.absolute_path(&self.codeowners_file_path)?.clean())
    }

    fn absolute_path(&self, path: &Path) -> Result<PathBuf, RunnerError> {
        Ok(self.absolute_project_root()?.join(path))
    }
}

pub fn cli() -> Result<RunResult, RunnerError> {
    let args = Args::parse();

    let config_path = args.absolute_config_path()?;
    let codeowners_file_path = args.absolute_codeowners_path()?;
    let project_root = args.absolute_project_root()?;

    let run_config = RunConfig {
        config_path,
        codeowners_file_path,
        project_root,
        no_cache: args.no_cache,
    };

    let runner = Runner::new(&run_config)?;

    let runner_result = match args.command {
        Command::Validate => runner.validate(),
        Command::Generate => runner.generate(),
        Command::GenerateAndValidate => runner.generate_and_validate(),
        Command::ForFile { name } => runner.for_file(&name),
        Command::ForTeam { name } => runner.for_team(&name),
        Command::DeleteCache => runner.delete_cache(),
    };

    Ok(runner_result)
}
