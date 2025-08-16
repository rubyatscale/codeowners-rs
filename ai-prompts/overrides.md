# Owner overrides

## Context

Today, if more than one mapper claims ownership of a file, we return an error. There are limited exceptions for directory ownership where multiple directories may apply, and we conceptually pick the nearest directory.

We have seen frequent confusion and repeated requests for a way to explicitly override ownership.

## Proposal

Allow overrides, resolving conflicts by choosing the most specific claim.

Priority (most specific to least specific)
Ownership for a file is determined by the closest applicable claim:

File annotations (inline annotations in the file)
Directory ownership (the nearest ancestor directory claim). If directory ownership is declared above a package file, it would be lower priority
Package ownership (package.yml, package.json)
Team glob patterns
If multiple teams match at the same priority level (e.g., multiple team glob patterns match the same path), this remains an error.

## Tooling to reduce confusion

Update the for-file command to list all matching teams in descending priority, so users can see which owner would win and why.

### generate-and-validate AND for-file

Both places need to be updated. There should be tests in place that verify consistency. **I can likely compare the src/parser.rs derived team to the for-file team** in tests locally and against a large repo.

## The details

### gv
- implement the new errors
- sort a file's "owners" by priority when writing to CODEOWNERS use the most specific...I think this means we can't have redundancy

### for-file
- same errors? Look for reuse
- descriptive results
- gem should have a verbose option

### "New" errors

1. FileWithMultipleOwners is still a thing. Probably only possible if more than one team file claims ownership. Reason being is that other "claims" can be prioritized by proximity to the file
1. RedundantOwnership. **Looks like redundancy is already OK**. Maybe? Would we want avoid files getting owned by the same team multiple ways? It could potentially not be a problem with a really good `for-file`, but ... It's important to keep in mind that it's better to start more restrictive and then relax constraints than the other way around.

### Questions
1. Why is owner's source a vec? 
Looks like every claim for the same team goes in there. As an example, you can add a matching file annotation and there will be no error and for-file will show all sources