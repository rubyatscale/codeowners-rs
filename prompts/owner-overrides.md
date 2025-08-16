# Feature Request

## Owner overrides

### Context

Today, if more than one mapper claims ownership of a file, we return an error. There are limited exceptions for directory ownership where multiple directories may apply, and we conceptually pick the nearest directory.

We have seen frequent confusion and repeated requests for a way to explicitly override ownership.

### Proposal

Allow overrides, resolving conflicts by choosing the most specific claim.

#### Priority (most specific to least specific)

Ownership for a file is determined by the _closest_ applicable claim:

1. File annotations (inline annotations in the file)
1. *Directory ownership (the nearest ancestor directory claim)
1. Package ownership (`package.yml`, `package.json`)
1. Team glob patterns

If multiple teams match at the same priority level (e.g., multiple team glob patterns match the same path), this remains an error.

#### Tooling to reduce confusion

Update the `for-file` command to list all matching teams in descending priority, so users can see which owner would win and why.


* If directory ownership is declared _above_ a package file, it would be lower priority