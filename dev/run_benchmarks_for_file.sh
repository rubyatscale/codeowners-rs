#!/bin/bash

mkdir -p tmp

# Check if the file exists before removing it
if [ -f "tmp/codeowners_for_file_benchmarks.md" ]; then
  rm tmp/codeowners_for_file_benchmarks.md
fi

echo "To run these benchmarks on your application, you can place this repo next to your rails application and run /usr/bin/env bash ../rubyatscale/codeowners-rs/dev/run_benchmarks_for_file.sh <path/to/file>" >> tmp/codeowners_for_file_benchmarks.md

hyperfine --warmup=2 --runs=3 --export-markdown tmp/codeowners_for_file_benchmarks.md \
  "../rubyatscale/codeowners-rs/target/release/codeowners for-file \"$1\"" \
  "bin/codeowners for_file \"$1\"" \
  "bin/codeownership for_file \"$1\"" 