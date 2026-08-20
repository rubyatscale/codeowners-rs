# Performance harness

Measures `validate`, `generate` and `generate-and-validate` so performance claims
can be checked instead of asserted.

This is a **local development tool**. It is deliberately not wired into CI: shared
runners are too noisy for the 2–20 second wall-clock comparisons that matter here,
and they have no large corpus to measure against. Performance regressions are
caught when someone runs this on purpose.

## Quick start

```bash
# Smoke test — proves the harness works. NOT a measurement.
./perf/run.sh

# Real numbers.
export CODEOWNERS_PERF_CORPUS=/path/to/a/large/monorepo
./perf/run.sh
```

## The default corpus is a smoke test, not a measurement

With no corpus configured the harness runs against `tests/fixtures/valid_project`
— 28 files and a 41-line CODEOWNERS. Every case finishes in single-digit
milliseconds and they all look about the same.

That is genuinely useful: it proves the cases execute, the JSON is well-formed,
snapshot/restore fires, and the file-count assertions hold. It is useless for
comparing optimizations.

**To get numbers that mean anything, point the harness at a large monorepo.** A
useful corpus has on the order of 10⁵ tracked files and a CODEOWNERS file with
thousands of entries; that is the shape where the interesting costs show up.

Two guards exist so nobody mistakes one for the other:

1. `run.sh` prints a loud banner when the corpus has fewer than 1,000 tracked
   files, naming the corpus and its size.
2. `compare.sh` **refuses** to diff two reports whose corpus path or corpus git
   commit differ. This catches the subtler mistake — a branch measured on the
   fixture diffed against a baseline measured on the monorepo, which would
   otherwise read as a spectacular speedup.

Every report records the corpus path, its commit, its tracked-file count and its
CODEOWNERS line count.

## Corpus resolution

First match wins:

1. `--corpus <path>`
2. `$CODEOWNERS_PERF_CORPUS`
3. `tests/fixtures/valid_project` (committed fixture)

No path to any specific monorepo is stored in this repository. The corpus must
contain a readable `config/code_ownership.yml`.

## Cases

| Case | Command under test | What it isolates |
| --- | --- | --- |
| `generate` | `generate` | Project build + one file generation |
| `validate_all` | `validate` | Full ownership validation |
| `gv` | `generate-and-validate` | The headline CI/pre-commit-hook command |
| `gv_files_100` | `gv <100 paths>` | The likely real hook invocation |
| `gv_files_1000` | `gv <1000 paths>` | Same, larger changeset |
| `validate_all_cold` | `validate --no-cache` | Guards against wins that only exist warm |
| `validate_files_1` | `validate <1 path>` | Fixed-cost floor |
| `validate_files_100` | `validate <100 paths>` | Realistic changeset |
| `validate_files_1000` | `validate <1000 paths>` | Exposes any per-file linear term |
| `validate_files_2000` | `validate <2000 paths>` | Confirms the slope |

`codeowners-perf cases` lists them. `--case <substring>` filters.

Cases needing more owned files than the corpus contains are reported as
**skipped** with the reason, never silently shrunk.

**`gv <paths>` and `validate <paths>` are not the same measurement.** `generate`
needs the project build, so an optimization that bypasses that build speeds up
`validate <paths>` but can do nothing for `gv <paths>`. Both are measured because
a hook that runs `gv` sees the smaller of the two wins, and quoting the
`validate` number for it would be wrong.

## Reading the output

- **best** is the headline number. **median** is shown alongside so you can see
  whether a run was noisy; all individual run times are kept in the JSON.
- `compare` reports the observed run-to-run **spread** per case and marks any
  delta smaller than it **within noise**. Trust that column over the delta:
  min-of-N is a biased estimator with no dispersion attached, so a 3% "win" on a
  case that swings 40% between runs reads exactly like a real one.
- `validate_all_cold` is by far the noisiest case — it is IO-bound and a 50%
  spread between runs is normal, which is larger than most effects worth hunting.
  It is useful as a guard against wins that only exist warm, not as a number to
  optimize against. Raise `--runs` a lot if you need to trust it.

### Two things the numbers do not include

- **Per-invocation setup is undercounted.** All cases run in one process, and
  `teams_by_github_team_name` is `#[memoize]`d process-globally, so the warmup run
  pays the team-file parse and no timed run ever does. A real CLI invocation pays
  it every time. Treat published numbers as a floor for single-shot CLI cost.
- **Fixed cost dominates small changesets.** The per-file cases are affine, not
  proportional: on a 130k-file corpus they fit ~2.0s fixed plus ~9.9ms/file. So
  the per-file *average* is ~2,100ms at one file and ~11ms at two thousand. For
  the common CI case — a PR touching a handful of files — essentially all of the
  time is the fixed project build, and the per-file rate is nearly irrelevant.
  Quote both terms, or you will optimize the wrong end.

### Phase percentages: check nesting before quoting

Spans are inclusive of children, so sibling spans can be summed and nested ones
cannot. `config_load`, `cache_init`, `project_build`, `cache_persist` and
`per_file_query` are disjoint — percentages across those are sound. But
`ownership_validate` ⊃ `validator_validate` ⊃ `validate_file_ownership` ⊃
`file_to_owners`: quoting those together as shares of one total double-counts.
- The **phase breakdown** comes from `tracing` spans inside the library. Phases
  are **inclusive of nested children**, so they do not sum to the total —
  `project_build` contains its own sub-work, and `ownership_validate` contains
  `validator_validate`, which contains `file_to_owners`. Use them to attribute a
  win, not to reconstruct a total.
- `mapper_build` is **accumulated across all calls in a run**. That is
  deliberate: it is currently invoked more than once per validate, and the sum is
  what a fix should reduce.

## The corpus is written to, and restored

`generate` and `generate-and-validate` **write** the corpus's CODEOWNERS file.
The harness snapshots that file before running and restores it afterwards.

It also **refuses to start** if the corpus's CODEOWNERS already has uncommitted
changes — otherwise it could not tell its own writes from yours, and restoring
would clobber your work. Commit or stash first.

## Adding a case

Add an entry to `CASES` in `src/bin/codeowners-perf.rs`. A case is a name, a
command kind, a file count and a cache flag. `tests/perf_harness_test.rs` covers
the harness mechanics, so run `cargo test --test perf_harness_test` afterwards.

## No baseline is committed — you generate your own

There is deliberately no `perf/baseline.json` in the repository. Two reasons:

1. **Wall-clock numbers are not portable.** A baseline measured on one laptop says
   nothing about another machine, so a committed one would invite exactly the
   invalid comparison the guards above exist to prevent. Reports record
   `machine` (os/arch/cpu count) and `compare.sh` warns when it differs.
2. **It would embed a local absolute path.** The corpus path is recorded in every
   report to make comparisons safe; committing one would put somebody's private
   checkout path into the repo.

`perf/results/` is gitignored. Measure the base branch yourself, then your branch,
on the same machine and the same corpus.

## Comparing a branch

```bash
export CODEOWNERS_PERF_CORPUS=/path/to/a/large/monorepo

# 1. baseline: on the branch you are comparing against
git checkout main
./perf/run.sh --json > perf/results/base.json

# 2. candidate: your branch
git checkout my-branch
./perf/run.sh --json > perf/results/my-branch.json

# 3. diff
./perf/compare.sh perf/results/base.json perf/results/my-branch.json
```

Paste the resulting table into the PR. **Report regressions too**, including on
`validate_all_cold`.

Before claiming a speedup, verify correctness — a faster wrong answer is the main
risk in this area:

```bash
# byte-identical generated output vs. the base branch
./target/release/codeowners --project-root "$CODEOWNERS_PERF_CORPUS" g -s
cp "$CODEOWNERS_PERF_CORPUS/.github/CODEOWNERS" /tmp/after.txt
git stash && cargo build --release   # or check out the base branch
./target/release/codeowners --project-root "$CODEOWNERS_PERF_CORPUS" g -s
diff /tmp/after.txt "$CODEOWNERS_PERF_CORPUS/.github/CODEOWNERS" && echo "identical"
```

## A trap worth knowing about

While profiling this originally, the CLI reported a suspiciously flat ~2.2s for 1,
100, 1,000 and 5,000 files. The cause was the shell, not the code: **zsh does not
word-split unquoted variables**, so `codeowners v $FILES` passed one
newline-joined mega-argument. It matched no glob, was filtered out, and the
per-file loop never ran. The measurement looked clean and was measuring nothing.

Two consequences for this harness:

- It builds argument lists as real vectors in Rust, never by interpolating a
  shell string.
- It asserts that each case built exactly the number of paths it asked for, so a
  future filtering regression fails loudly instead of producing a fast number.

If you time the CLI by hand, use an array: `"${FILES[@]}"`.
