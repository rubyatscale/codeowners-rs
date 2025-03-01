mod cli;
use std::process;

use codeowners::runner::{Error as RunnerError, RunResult};
use error_stack::Result;

use crate::cli::cli;

fn main() -> Result<(), RunnerError> {
    install_logger();
    maybe_print_errors(cli()?)?;

    Ok(())
}

fn maybe_print_errors(result: RunResult) -> Result<(), RunnerError> {
    if !result.info_messages.is_empty() {
        for msg in result.info_messages {
            println!("{}", msg);
        }
    }
    if !result.io_errors.is_empty() || !result.validation_errors.is_empty() {
        for msg in result.io_errors {
            eprintln!("{}", msg);
        }
        for msg in result.validation_errors {
            println!("{}", msg);
        }
        process::exit(-1);
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
