# Codeowners
Codeowners is a CLI tool written in Rust that can generate Github's `CODEOWNERS` file. It's a re-implementation of an existing [tool](https://github.com/rubyatscale/code_ownership).

Codeowners assumes certain conventions around Ruby/Javascript packages. These can probably be abstracted away to be language agnostic but are currently not.

The sole purpose of this tool existing is speed. `CODEOWNERS` generation happens as part of our git commit hooks and has on Gusto's main repo takes 18s to run. The Rust implementation which is a drop in replacement cuts that down to <= 2s. (Tested on a Mackbook M1)

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
