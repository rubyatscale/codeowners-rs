mod cli;
use std::process;

use cli::Error;
use codeowners::ownership;
use error_stack::Result;

use crate::cli::cli;

fn main() -> Result<(), Error> {
    install_logger();
    maybe_print_errors(cli())?;

    Ok(())
}

fn maybe_print_errors(result: Result<(), Error>) -> Result<(), Error> {
    if let Err(error) = result {
        if let Some(validation_errors) = error.downcast_ref::<ownership::ValidatorErrors>() {
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
