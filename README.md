# Codeowners

**Codeowners** is a fast, Rust-based CLI for generating and validating [GitHub `CODEOWNERS` files](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners) in large repositories. It supports conventions for Ruby and JavaScript projects, and is a high-performance reimplementation of the [original Ruby CLI](https://github.com/rubyatscale/code_ownership).

## 🚀 Quick Start: Generate & Validate

The most common workflow is to **generate and validate your CODEOWNERS file** in a single step:

```sh
codeowners gv
```

- This command will:
  - Generate a fresh `CODEOWNERS` file (by default at `.github/CODEOWNERS`)
  - Validate that all files are properly owned and that the file is up to date
  - Exit with a nonzero code and detailed errors if validation fails

**Why use this tool?**  
On large projects, `codeowners gv` is _over 10x faster_ than the legacy Ruby implementation:

```
$ hyperfine 'codeownership validate' 'codeowners validate'
Benchmark 1: codeownership validate (ruby gem)
  Time (mean ± σ):     47.991 s ±  1.220 s
Benchmark 2: codeowners gv (this repo)
  Time (mean ± σ):      4.263 s ±  0.025 s

Summary
  codeowners gv ran 11.26 ± 0.29 times faster than codeownership validate
```

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
