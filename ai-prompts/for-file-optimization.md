## Re-architect `codeowners-rs`

The CLI behaviors live in `src/cli.rs`.

### What this tool does
- **Generate CODEOWNERS**: Builds `.github/CODEOWNERS` from multiple ownership sources. See mappers in `src/ownership/mapper/`.
- **Answer per-file ownership**: The `for-file` command returns the owner of a given file even if the checked-in `CODEOWNERS` is not up to date.

### Current implementations
- **Fast but potentially inaccurate**: `Runner::for_file_from_codeowners` (in `src/runner.rs`) parses the existing `CODEOWNERS` file to find a file’s owner. It’s very fast, but can be wrong if `CODEOWNERS` is stale.
- **Accurate but slow**: `Runner::for_file` is correct but can take up to ~4 seconds on large repositories because it effectively determines ownership for many files and then returns the single result needed.

Ownership resolution is complex because definitions often live in other files (packages, team configs, globs, etc.).

## Assignment
Implement a faster `for_file` that remains accurate.

### Requirements
- **Performance**: Under 1 second on large codebases (e.g., `$HOME/workspace/large`).
- **Correctness**: All existing tests must continue to pass.
- **Compatibility**: No changes to the external behavior of the `for-file` CLI command.