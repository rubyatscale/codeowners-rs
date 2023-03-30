# Codeowners
The oxidation of an (existing CLI)[https://github.com/rubyatscale/code_ownership] written in Ruby.
This tool generates Github's `CODEOWNERS` file, assuming certain conventions around Ruby/Javascript packages.

`CODEOWNERS` generation happens as part of our git commit hooks and on Gusto's main repo takes ~18s to run. The Rust implementation which is a drop in replacement cuts that down to <= 2s. (Tested on a Mackbook M1)

```
A CLI to validate and generate Github's CODEOWNERS file

Usage: codeowners [OPTIONS] <COMMAND>

Commands:
  generate               Generate the CODEOWNERS file and save it to '--codeowners-file-path'
  validate               Validate the validity of the CODEOWNERS file. A validation failure will exit with a failure code and a detailed output of the validation errors
  generate-and-validate  Chains both 'generate' and 'verify' commands
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

### Development
The CLI is written in Rust. Rust provides strong type gurantees and an great ecosystem of CLI libraries. To be able to compile the code locally, you'll need to setup a rust compiler (See https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
