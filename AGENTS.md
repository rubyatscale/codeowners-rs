This file provides guidance to AI coding agents when working with code in this repository.

## What this project is

`codeowners-rs` is a Rust implementation of a CODEOWNERS resolver. It parses CODEOWNERS files and determines ownership of files in a repository, used as both a library and CLI tool.

## Commands

```bash
# Build
cargo build
cargo build --release

# Run all tests
cargo test

# Run a single test by name
cargo test test_name

# Lint
cargo clippy --all-targets --all-features
cargo fmt --all -- --check   # check only
cargo fmt --all               # apply formatting

# Check compilation without building
cargo check
```

## Architecture

- `src/main.rs` — CLI entry point using `clap`
- `src/lib.rs` — library root; exposes the public API
- `src/ownership.rs` / `src/ownership/` — core ownership resolution logic: parses CODEOWNERS patterns and matches them against file paths
- `src/config.rs` — configuration loading
- `src/runner/` — orchestrates file walking and ownership resolution
- `src/cache/` — caching layer for resolved ownership results
- `src/cli.rs` — CLI command definitions
- Integration tests in `src/common_test.rs`; fixture data used in tests lives alongside test files
