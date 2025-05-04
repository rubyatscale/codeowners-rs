# Codeowners

This is a CLI tool to generate [GitHub `CODEOWNERS` files](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners) from an existing project assuming certain conventions around file annotations and Ruby/Javascript packages.
It's also the [oxidation](https://wiki.mozilla.org/Oxidation) of an existing [CLI tool](https://github.com/rubyatscale/code_ownership) written in Ruby.

Targeting a large project, `codeowners` can be signifcantly faster than [code_ownership](https://github.com/rubyatscale/code_ownership):

``` text
$ hyperfine 'codeownership validate' 'codeowners validate'
Benchmark 1: codeownership validate (ruby gem https://github.com/rubyatscale/code_ownership)
  Time (mean ± σ):     47.991 s ±  1.220 s    [User: 4.590 s, System: 24.379 s]
  Range (min … max):   46.777 s … 49.217 s    3 runs

Benchmark 2: codeowners gv (this repo)
  Time (mean ± σ):      4.263 s ±  0.025 s    [User: 9.359 s, System: 3.273 s]
  Range (min … max):    4.246 s …  4.292 s    3 runs

Summary
  codeowners gv ran
   11.26 ± 0.29 times faster than codeownership validate
```

| Command | Mean [s] | Min [s] | Max [s] | Relative |
|:---|---:|---:|---:|---:|
| `codeownership validate` | 47.991 ± 1.220 | 46.777 | 49.217 | 11.26 ± 0.29 |
| `codeowners gv` | 4.263 ± 0.025 | 4.246 | 4.292 | 1.00 |

## Documentation

```text
A CLI to validate and generate Github's CODEOWNERS file

Usage: codeowners [OPTIONS] <COMMAND>

Commands:
  for-file               Finds the owner of a given file. [aliases: f]
  for-team               Finds code ownership information for a given team  [aliases: t]
  generate               Generate the CODEOWNERS file and save it to '--codeowners-file-path'. [aliases: g]
  validate               Validate the validity of the CODEOWNERS file. A validation failure will exit with a failure code and a detailed output of the validation errors. [aliases: v]
  generate-and-validate  Chains both `generate` and `validate` commands. [aliases: gv]
  help                   Print this message or the help of the given subcommand(s)

Options:
      --codeowners-file-path <CODEOWNERS_FILE_PATH>
          Path for the CODEOWNERS file [default: ./.github/CODEOWNERS]
      --config-path <CONFIG_PATH>
          Path for the configuration file [default: ./config/code_ownership.yml]
      --project-root <PROJECT_ROOT>
          Path for the root of the project [default: .]
  -h, --help
          Print help
  -V, --version
          Print version
```

### Getting Started

See the [CONFIGURATION.md](CONFIGURATION.md) file for more information on how to configure codeowners.

#### for-file

Finds the owner of a given file.

``` text
$ codeowners for-file javascript/packages/PayrollFlow/index.tsx

Team: Payroll
Team YML: config/teams/payroll.yml
Description: 
- Owner annotation at the top of the file.
- Owner defined in `javascript/packages/PayrollFlow/package.json` with implicity owned glob: `javascript/packages/PayrollFlow/**/**`

```

#### for-team

Finds code ownership information for a given team.

``` text
$ codeowners for-team Payroll

# Code Ownership Report for `Payroll` Team

## Annotations at the top of file
/javascript/packages/PayrollFlow/index.tsx
/ruby/app/models/payroll.rb

## Team-specific owned globs
This team owns nothing in this category.

## Owner in .codeowner
/ruby/app/payroll/**/**

## Owner metadata key in package.yml
/ruby/packages/payroll_flow/**/**

## Owner metadata key in package.json
/javascript/packages/PayrollFlow/**/**

## Team YML ownership
/config/teams/payroll.yml

## Team owned gems
/gems/payroll_calculator/**/**
```

### Development

The CLI is written in Rust. Rust provides strong type gurantees and an great ecosystem of CLI libraries. To be able to compile the code locally, you'll need to setup a rust compiler (See https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
