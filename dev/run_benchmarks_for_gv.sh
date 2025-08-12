#!/bin/bash

# Check if the file exists before removing it
if [ -f "tmp/codeowners_benchmarks.md" ]; then
  rm tmp/codeowners_benchmarks_gv.md
fi

echo "To run these benchmarks on your application, you can place this repo next to your rails application and run /usr/bin/env bash ../rubyatscale/codeowners-rs/dev/run_benchmarks_for_gv.sh from the root of your application" >> tmp/codeowners_benchmarks_gv.md

hyperfine --warmup=2 --runs=3 --export-markdown tmp/codeowners_benchmarks_gv.md \
  '../rubyatscale/codeowners-rs/target/release/codeowners gv' \
  'bin/codeowners validate' \
  'bin/codeownership validate' 