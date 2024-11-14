#!/bin/bash

# Check if the file exists before removing it
if [ -f "tmp/codeowners_benchmarks.md" ]; then
  rm tmp/codeowners_benchmarks.md
fi

echo "To run these benchmarks on your application, you can place this repo next to your rails application and run bash ../rubyatscale/codeowners-rs/dev/run_benchmarks.sh from the root of your application" >> tmp/codeowners_benchmarks.md

hyperfine --warmup=2 --runs=3 --export-markdown tmp/codeowners_benchmarks.md \
  '../rubyatscale/codeowners-rs/target/release/codeowners gv' \
  'bin/codeowners-rs gv' 