# Codeowners

**Codeowners** is a fast, Rust-based CLI for generating and validating [GitHub `CODEOWNERS` files](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners) in large repositories. 

Note: For Ruby application, it's usually  easier to use `codeowners-rs` via the [code_ownership](https://github.com/rubyatscale/code_ownership) gem.

## 🚀 Quick Start: Generate & Validate

The most common workflow is to **generate and validate your CODEOWNERS file** in a single step:

```sh
codeowners gv
```

- This command will:
  - Generate a fresh `CODEOWNERS` file (by default at `.github/CODEOWNERS`)
  - Validate that all files are properly owned and that the file is up to date
  - Exit with a nonzero code and detailed errors if validation fails

## Table of Contents

- [Quick Start: Generate & Validate](#-quick-start-generate--validate)
- [Getting Started](#getting-started)
- [Declaring Ownership](#declaring-ownership)
  - [Directory-Based Ownership](#1-directory-based-ownership)
  - [File Annotation](#2-file-annotation)
  - [Package-Based Ownership](#3-package-based-ownership)
  - [Glob-Based Ownership](#4-glob-based-ownership)
  - [JavaScript Package Ownership](#5-javascript-package-ownership)
- [Other CLI Commands](#other-cli-commands)
  - [Examples](#examples)
    - [Find the owner of a file](#find-the-owner-of-a-file)
    - [Ownership report for a team](#ownership-report-for-a-team)
- [Validation](#validation)
- [Development](#development)
- [Configuration](#configuration)

## Getting Started

1. **Install DotSlash**  
   [Install DotSlash](https://dotslash-cli.com/docs/installation/)  
   Releases include a DotSlash text file that will automatically download and run the correct binary for your system.

2. **Download the Latest DotSlash Text File**  
   Releases contain a DotSlash text file. Example: [codeowners release v0.2.4](https://github.com/rubyatscale/codeowners-rs/releases/download/v0.2.4/codeowners).  
   Running this file with DotSlash installed will execute `codeowners`.

3. **Configure Ownership**  
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

4. **Declare Teams**  
   Example: `config/teams/operations.yml`

   ```yaml
   name: Operations
   github:
     team: '@my-org/operations-team'
   ```

5. **Run the Main Workflow**

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


## Other CLI Commands

While `codeowners gv` is the main workflow, the CLI also supports:

```text
Usage: codeowners [OPTIONS] <COMMAND>

Commands:
  for-file               Finds the owner of a given file. [aliases: f]
  for-team               Finds code ownership information for a given team [aliases: t]
  generate               Generate the CODEOWNERS file [aliases: g]
  validate               Validate the CODEOWNERS file [aliases: v]
  generate-and-validate  Chains both `generate` and `validate` [aliases: gv]
  help                   Print this message or the help of the given subcommand(s)

Options:
      --codeowners-file-path <CODEOWNERS_FILE_PATH>  [default: ./.github/CODEOWNERS]
      --config-path <CONFIG_PATH>                    [default: ./config/code_ownership.yml]
      --project-root <PROJECT_ROOT>                  [default: .]
  -h, --help
  -V, --version
```

### Examples

#### Find the owner of a file

```sh
codeowners for-file path/to/file.rb
```

#### Ownership report for a team

```sh
codeowners for-team Payroll
```

## Validation

`codeowners validate` (or `codeowners gv`) ensures:

1. Only one mechanism defines ownership for any file.
2. All referenced teams are valid.
3. All files in `owned_globs` are owned, unless in `unowned_globs`.
4. The `CODEOWNERS` file is up to date.

## Development

- Written in Rust for speed and reliability.
- To build locally, install Rust:  

  ```sh
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```

- Please update `CHANGELOG.md` and this `README.md` when making changes.

### Module layout (for library users)

- `src/runner.rs`: public façade re-exporting the API and types.
- `src/runner/api.rs`: externally available functions used by the CLI and other crates.
- `src/runner/types.rs`: `RunConfig`, `RunResult`, and runner `Error`.
- `src/ownership/`: all ownership logic (parsing, mapping, validation, generation).
- `src/ownership/codeowners_query.rs`: CODEOWNERS-only queries consumed by the façade.

Import public APIs from `codeowners::runner::*`.

### Library usage example

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
