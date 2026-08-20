#!/usr/bin/env bash
#
# Driver for the codeowners performance harness.
#
# Builds the release harness and runs the benchmark cases. All the real work
# (corpus resolution, snapshot/restore, case timing, JSON) lives in
# src/bin/codeowners-perf.rs — this script just makes the common invocation
# short and keeps you from accidentally measuring a debug build.
#
# Usage:
#   ./perf/run.sh                                   # fixture corpus (smoke test)
#   ./perf/run.sh --corpus /path/to/large-monorepo   # real numbers
#   ./perf/run.sh --json > perf/results/mine.json    # machine readable
#   ./perf/run.sh --case gv --runs 5
#
# See perf/README.md.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

# A debug build is 10-30x slower and its numbers are meaningless for comparison,
# so the release build is not optional.
echo "building release harness..." >&2
cargo build --release --bin codeowners-perf --quiet

exec ./target/release/codeowners-perf run "$@"
