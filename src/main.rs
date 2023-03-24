use std::{error::Error, fs::File, path::Path};

use tracing::debug;

use crate::{config::Config, ownership::Ownership, project::Project};

mod config;
mod ownership;
mod project;

fn main() -> Result<(), Box<dyn Error>> {
    install_logger();

    let config: Config = serde_yaml::from_reader(File::open("./.config.yml")?)?;
    let base_path = Path::new("../zenpayroll");

    debug!("Project::build()");
    let project = Project::build(base_path, &config.compile())?;

    debug!("Ownership::build()");
    let ownership = Ownership::build(project);

    debug!("Ownership.write_to_file()");
    ownership.write_to_file(Path::new("../zenpayroll/.github/CODEOWNERS"))?;

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
