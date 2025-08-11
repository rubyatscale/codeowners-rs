// This is a tool to compare the output of the original codeowners CLI with the optimized version.
// It's useful for verifying that the optimized version is correct.
//
// It's not used in CI, but it's useful for debugging.
//
// To run it, use `cargo run --bin compare_for_file <absolute_project_root>`
//
// It will compare the output of the original codeowners CLI with the optimized version for all files in the project.

use std::{
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use codeowners::config::Config as OwnershipConfig;
use codeowners::ownership::{FileOwner, for_file_fast};
use codeowners::runner::{RunConfig, Runner};
use ignore::WalkBuilder;

fn main() {
    let project_root = std::env::args().nth(1).expect("usage: compare_for_file <absolute_project_root>");
    let project_root = PathBuf::from(project_root);
    if !project_root.is_absolute() {
        eprintln!("Project root must be absolute");
        std::process::exit(2);
    }

    let codeowners_file_path = project_root.join(".github/CODEOWNERS");
    let config_path = project_root.join("config/code_ownership.yml");

    let run_config = RunConfig {
        project_root: project_root.clone(),
        codeowners_file_path,
        config_path: config_path.clone(),
        no_cache: false,
    };

    // Build the original, accurate-but-slower runner once
    let runner = match Runner::new(&run_config) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to initialize Runner: {}", e);
            std::process::exit(1);
        }
    };

    // Load config once for the optimized path
    let config_file = match File::open(&config_path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Can't open config file {}: {}", config_path.display(), e);
            std::process::exit(1);
        }
    };
    let optimized_config: OwnershipConfig = match serde_yaml::from_reader(config_file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Can't parse config file {}: {}", config_path.display(), e);
            std::process::exit(1);
        }
    };

    let mut total_files: usize = 0;
    let mut diff_count: usize = 0;

    // Prefer tracked files from git; fall back to walking the FS if git is unavailable
    let tracked_files_output = Command::new("git").arg("-C").arg(&project_root).arg("ls-files").arg("-z").output();

    match tracked_files_output {
        Ok(output) if output.status.success() => {
            let bytes = output.stdout;
            for rel in bytes.split(|b| *b == 0u8) {
                if rel.is_empty() {
                    continue;
                }
                let rel_str = match std::str::from_utf8(rel) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let abs_path = project_root.join(rel_str);
                // Only process regular files that currently exist
                if !abs_path.is_file() {
                    continue;
                }

                total_files += 1;
                let original = run_original(&runner, &abs_path);
                let optimized = run_optimized(&project_root, &optimized_config, &abs_path);

                if original != optimized {
                    diff_count += 1;
                    println!("\n==== {} ====", abs_path.display());
                    println!("ORIGINAL:\n{}", original);
                    println!("OPTIMIZED:\n{}", optimized);
                    let _ = io::stdout().flush();
                }

                if total_files % 1000 == 0 {
                    eprintln!("Processed {} files... diffs so far: {}", total_files, diff_count);
                }
            }
        }
        _ => {
            eprintln!("git ls-files failed; falling back to filesystem walk (untracked files may be included)");
            let walker = WalkBuilder::new(&project_root)
                .hidden(false)
                .git_ignore(true)
                .git_exclude(true)
                .follow_links(false)
                .build();

            for result in walker {
                let entry = match result {
                    Ok(e) => e,
                    Err(err) => {
                        eprintln!("walk error: {}", err);
                        continue;
                    }
                };
                if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                    continue;
                }
                let path = entry.path();
                total_files += 1;

                let original = run_original(&runner, path);
                let optimized = run_optimized(&project_root, &optimized_config, path);

                if original != optimized {
                    diff_count += 1;
                    println!("\n==== {} ====", path.display());
                    println!("ORIGINAL:\n{}", original);
                    println!("OPTIMIZED:\n{}", optimized);
                    let _ = io::stdout().flush();
                }

                if total_files % 1000 == 0 {
                    eprintln!("Processed {} files... diffs so far: {}", total_files, diff_count);
                }
            }
        }
    }

    println!("Checked {} files. Diffs: {}", total_files, diff_count);
    if diff_count > 0 {
        std::process::exit(3);
    }
}

fn run_original(runner: &Runner, file_path: &Path) -> String {
    let result = runner.for_file(&file_path.to_string_lossy());
    if !result.validation_errors.is_empty() {
        return result.validation_errors.join("\n");
    }
    if !result.io_errors.is_empty() {
        return format!("IO_ERROR: {}", result.io_errors.join(" | "));
    }
    result.info_messages.join("\n")
}

fn run_optimized(project_root: &Path, config: &OwnershipConfig, file_path: &Path) -> String {
    let owners: Vec<FileOwner> = match for_file_fast::find_file_owners(project_root, config, file_path) {
        Ok(v) => v,
        Err(e) => return format!("IO_ERROR: {}", e),
    };
    match owners.len() {
        0 => format!("{}", FileOwner::default()),
        1 => format!("{}", owners[0]),
        _ => {
            let mut lines = vec!["Error: file is owned by multiple teams!".to_string()];
            for owner in owners {
                lines.push(format!("\n{}", owner));
            }
            lines.join("\n")
        }
    }
}