# Codeowners

**Codeowners** is a fast, Rust-based CLI for generating and validating [GitHub `CODEOWNERS` files](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners) in large repositories.

Note: For Ruby applications, it's usually easier to use `codeowners-rs` via the [code_ownership](https://github.com/rubyatscale/code_ownership) gem.

## 🚀 Quick Start: Generate & Validate

The most common workflow is to generate and validate in one step:

```sh
codeowners gv
```

- Generates a fresh `CODEOWNERS` file (default: `.github/CODEOWNERS`)
- Validates ownership and that the file is up to date
- Exits non-zero and prints detailed errors if validation fails

## Table of Contents

- [Quick Start: Generate & Validate](#quick-start-generate--validate)
- [Installation](#installation)
- [Getting Started](#getting-started)
- [Declaring Ownership](#declaring-ownership)
  - [Directory-Based Ownership](#1-directory-based-ownership)
  - [File Annotation](#2-file-annotation)
  - [Package-Based Ownership](#3-package-based-ownership)
  - [Glob-Based Ownership](#4-glob-based-ownership)
  - [JavaScript Package Ownership](#5-javascript-package-ownership)
- [CLI Reference](#cli-reference)
  - [Global Flags](#global-flags)
  - [Commands](#commands)
  - [Examples](#examples)
- [Configuration](#configuration)
- [Cache](#cache)
- [Validation](#validation)
- [Library Usage](#library-usage)
- [Development](#development)

## Installation

You can run `codeowners` without installing a platform-specific binary by using DotSlash, or install from source with Cargo.

### Option A: DotSlash (recommended)

1. Install DotSlash: see [https://dotslash-cli.com/docs/installation/](https://dotslash-cli.com/docs/installation/)
2. Download the latest DotSlash text file from a release, for example [https://github.com/rubyatscale/codeowners-rs/releases](https://github.com/rubyatscale/codeowners-rs/releases).
3. Execute the downloaded file with DotSlash; it will fetch and run the correct binary.

### Option B: From source with Cargo

Requires Rust toolchain.

```sh
cargo install --git https://github.com/rubyatscale/codeowners-rs codeowners
```

## Getting Started

1. **Configure Ownership**  
   Create a `config/code_ownership.yml` file. Example:

   ```yaml
   owned_globs:
     - '{app,components,config,frontend,lib,packs,spec}/**/*.{rb,rake,js,jsx,ts,tsx}'
   js_package_paths: []
   unowned_globs:
     - db/**/*
     - app/services/some_file1.rb
     - frontend/javascripts/**/__generated__/**/*
   ```

2. **Declare Teams**  
   Example: `config/teams/operations.yml`

   ```yaml
   name: Operations
   github:
     team: '@my-org/operations-team'
   ```

3. **Run the Main Workflow**

   ```sh
   codeowners gv
   ```

## Declaring Ownership

You can declare code ownership in several ways:

### 1. Directory-Based Ownership

Add a `.codeowner` file to a directory with the team name as its contents:

```text
TeamName
```

### 2. File Annotation

Add an annotation at the top of a file:

```ruby
# @team MyTeam
```
```typescript
// @team MyTeam
```
```html
<!-- @team MyTeam -->
```
```erb
<%# @team: Foo %>
```
### 3. Package-Based Ownership

In `package.yml` (for Ruby Packwerk):

```yaml
owner: TeamName
```

### 4. Glob-Based Ownership

In your team's YML:

```yaml
owned_globs:
  - app/services/my_team/**/*
unowned_globs:
  - app/services/my_team/legacy/*
```

`unowned_globs` "subtracts" from `owned_globs`

### 5. JavaScript Package Ownership

In `package.json`:

```json
{
  "metadata": {
    "owner": "My Team"
  }
}
```

Configure search paths in `code_ownership.yml`:

```yaml
js_package_paths:
  - frontend/javascripts/packages/*
```

## CLI Reference

### Global Flags

- `--codeowners-file-path <path>`: Path for the CODEOWNERS file. Default: `./.github/CODEOWNERS`
- `--config-path <path>`: Path to `code_ownership.yml`. Default: `./config/code_ownership.yml`
- `--project-root <path>`: Project root. Default: `.`
- `--no-cache`: Disable on-disk caching (useful in CI)
- `-V, --version`, `-h, --help`

### Commands

- `generate` (`g`): Generate the CODEOWNERS file and write it to `--codeowners-file-path`.
  - Flags: `--skip-stage, -s` to avoid `git add` after writing
- `validate` (`v`): Validate the CODEOWNERS file and configuration.
- `generate-and-validate` (`gv`): Run `generate` then `validate`.
  - Flags: `--skip-stage, -s`
- `for-file <path>` (`f`): Print the owner of a file.
  - Flags: `--from-codeowners` to resolve using only the CODEOWNERS rules
- `for-team <name>` (`t`): Print ownership report for a team.
- `delete-cache` (`d`): Delete the persisted cache.

### Examples

#### Find the owner of a file

```sh
codeowners for-file path/to/file.rb
```

#### Ownership report for a team

```sh
codeowners for-team Payroll
```

#### Generate but do not stage the file

```sh
codeowners generate --skip-stage
```

#### Run without using the cache

```sh
codeowners gv --no-cache
```

## Configuration

`config/code_ownership.yml` keys and defaults:

- `owned_globs` (required): Glob patterns that must be owned.
- `ruby_package_paths` (default: `['packs/**/*', 'components/**']`)
- `js_package_paths` / `javascript_package_paths` (default: `['frontend/**/*']`)
- `team_file_glob` (default: `['config/teams/**/*.yml']`)
- `unowned_globs` (default: `['frontend/**/node_modules/**/*', 'frontend/**/__generated__/**/*']`)
- `vendored_gems_path` (default: `'vendored/'`)
- `cache_directory` (default: `'tmp/cache/codeowners'`)
- `ignore_dirs` (default includes: `.git`, `node_modules`, `tmp`, etc.)
- `executable_name` (default: `'codeowners'`): Customize the command name shown in validation error messages. Useful when using `codeowners-rs` via wrappers like the [code_ownership](https://github.com/rubyatscale/code_ownership) Ruby gem.

Example configuration with custom executable name:

```yaml
owned_globs:
  - '{app,components,config,frontend,lib,packs,spec}/**/*.{rb,rake,js,jsx,ts,tsx}'
executable_name: 'bin/codeownership'  # For Ruby gem wrapper
```

See examples in `tests/fixtures/**/config/` for reference setups.

## Cache

By default, cache is stored under `tmp/cache/codeowners` relative to the project root. This speeds up repeated runs.

- Disable cache for a run: add the global flag `--no-cache`
- Clear all cache: `codeowners delete-cache`

## Validation

`codeowners validate` (or `codeowners gv`) ensures:

1. Only one mechanism defines ownership for any file.
2. All referenced teams are valid.
3. All files in `owned_globs` are owned, unless matched by `unowned_globs`.
4. The generated `CODEOWNERS` file is up to date.

Exit status is non-zero on errors.

## Library Usage

Import public APIs from `codeowners::runner::*`.

```rust
use codeowners::runner::{RunConfig, for_file, teams_for_files_from_codeowners};

fn main() {
    let run_config = RunConfig {
        project_root: std::path::PathBuf::from("."),
        codeowners_file_path: std::path::PathBuf::from(".github/CODEOWNERS"),
        config_path: std::path::PathBuf::from("config/code_ownership.yml"),
        no_cache: true, // set false to enable on-disk caching
    };

    // Find owner for a single file using the optimized path (not just CODEOWNERS)
    let result = for_file(&run_config, "app/models/user.rb", false);
    for msg in result.info_messages { println!("{}", msg); }
    for err in result.io_errors { eprintln!("io: {}", err); }
    for err in result.validation_errors { eprintln!("validation: {}", err); }

    // Map multiple files to teams using CODEOWNERS rules only
    let files = vec![
        "app/models/user.rb".to_string(),
        "config/teams/payroll.yml".to_string(),
    ];
    match teams_for_files_from_codeowners(&run_config, &files) {
        Ok(map) => println!("{:?}", map),
        Err(e) => eprintln!("error: {}", e),
    }
}
```

## Development

- Written in Rust for speed and reliability.
- To build locally, install Rust:  

  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- Please update `CHANGELOG.md` and this `README.md` when making changes.

### Module layout

- `src/runner.rs`: public façade re-exporting the API and types.
- `src/runner/api.rs`: externally available functions used by the CLI and other crates.
- `src/runner/types.rs`: `RunConfig`, `RunResult`, and runner `Error`.
- `src/ownership/`: all ownership logic (parsing, mapping, validation, generation).
- `src/ownership/codeowners_query.rs`: CODEOWNERS-only queries consumed by the façade.
