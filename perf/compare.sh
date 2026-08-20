#!/usr/bin/env bash
#
# Diff two harness JSON reports into a markdown delta table, ready to paste into
# a PR description.
#
# Usage:
#   ./perf/compare.sh perf/baseline.json perf/results/my-branch.json
#
# Refuses to compare reports measured against different corpora or different
# corpus commits — see perf/README.md for why that matters.

set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <baseline.json> <candidate.json>" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo build --release --bin codeowners-perf --quiet
exec ./target/release/codeowners-perf compare "$1" "$2"
