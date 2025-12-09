# Bug Summary: `file_paths` Parameter in Validate

## Quick Reference

When `validate()` or `generate_and_validate()` is called with `file_paths`, it takes a completely different code path than validating all files, resulting in incomplete and inconsistent validation.

## The 5 Bugs

### 1. **Invalid Team Annotations Not Detected**
- **What**: Files with `# @team NonExistentTeam` reported as "Unowned" instead of "Invalid Team"
- **Why**: `validate_files()` only checks CODEOWNERS, doesn't validate team names exist

### 2. **Stale Team References After Rename**
- **What**: After renaming team in config/teams, files still referencing old name not flagged
- **Why**: Same root cause as Bug 1

### 3. **Multiple Ownership Not Detected**
- **What**: Files owned by both annotation AND package/directory/glob not flagged
- **Why**: `validate_files()` doesn't check all ownership sources via `find_file_owners()`

### 4. **Stale CODEOWNERS Not Detected**
- **What**: Manually modified or outdated CODEOWNERS file not caught
- **Why**: `validate_files()` skips the `validate_codeowners_file()` check

### 5. **Inconsistent Error Messages**
- **What**: Same issue reported differently depending on whether files specified
- **Why**: Combination of Bugs 1-4

## Root Cause

```rust
// validate_all() - CORRECT ✅
fn validate_all(&self) -> RunResult {
    self.ownership.validate()  // Runs comprehensive validation:
                                // - validate_invalid_team()
                                // - validate_file_ownership()
                                // - validate_codeowners_file()
}

// validate_files() - INCOMPLETE ❌
fn validate_files(&self, file_paths: Vec<String>) -> RunResult {
    // Only checks if file exists in CODEOWNERS
    team_for_file_from_codeowners(&self.run_config, &file_path)
    // Missing: team validity, multiple owners, stale CODEOWNERS
}
```

## Solution

Use the full validator but filter errors to specified files:

```rust
fn validate_files(&self, file_paths: Vec<String>) -> RunResult {
    // 1. Filter files by owned_globs/unowned_globs
    let filtered_paths = /* filter logic */;
    
    // 2. Run FULL validation (same as validate_all)
    let validator = Validator { /* ... */ };
    let all_errors = validator.validate();
    
    // 3. Filter errors to only those affecting our files
    let relevant_errors = all_errors.filter(|error| {
        match error {
            FileError(path) => filtered_paths.contains(path),
            GlobalError => true,  // Always include (e.g., stale CODEOWNERS)
        }
    });
    
    return relevant_errors;
}
```

## Tests

All bugs verified in `tests/bug_identification_test.rs`:

```bash
cargo test --test bug_identification_test -- --nocapture
```

Expected output: All 5 tests pass, each confirming the bug with `❌ BUG CONFIRMED` message.

## Files Changed

- ✅ `/tests/bug_identification_test.rs` - Comprehensive bug tests
- ✅ `/BUG_REPORT.md` - Detailed analysis
- ✅ `/BUG_SUMMARY.md` - This quick reference

## Next Steps

1. Review this analysis
2. Decide on implementation approach (Option A recommended)
3. Implement fix in `src/runner.rs`
4. Verify all tests pass
5. Update existing tests as needed

