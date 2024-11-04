# Codeowners
This is a CLI tool to generate [GitHub `CODEOWNERS` files](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners) from an existing project assuming certain conventions around file annotations and Ruby/Javascript packages.
It's also the [oxidation](https://wiki.mozilla.org/Oxidation) of an existing [CLI tool](https://github.com/rubyatscale/code_ownership) written in Ruby.

### Documentation

```bash
A CLI to validate and generate Github's CODEOWNERS file

Usage: codeowners [OPTIONS] <COMMAND>

Commands:
  generate               Generate the CODEOWNERS file and save it to '--codeowners-file-path'
  validate               Validate the validity of the CODEOWNERS file. A validation failure will exit with a failure code and a detailed output of the validation errors
  generate-and-validate  Chains both 'generate' and 'validate' commands
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

### Adoption
This is an experimental port that might never be supported, use at your own risk and be prepared to fallback to the Ruby implementation if it stops working, if you still wish to adopt it, here are the instructions:

```bash
# sets up a Rust toolchain
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Compile and install locally
gh repo clone rubyatscale/codeowners-rs
cd codeowners-rs
cargo install --path .

# Set an environment variable that switches ZP's commit hook to use the globally installed binary
echo USE_CODEOWNERS_RS=true >> ~/.zshrc
```

### Development
The CLI is written in Rust. Rust provides strong type gurantees and an great ecosystem of CLI libraries. To be able to compile the code locally, you'll need to setup a rust compiler (See https://rustup.rs/):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
