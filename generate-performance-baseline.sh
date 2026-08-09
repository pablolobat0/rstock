#!/usr/bin/env bash
set -euo pipefail

# The benchmark executable is the source of truth. Criterion writes raw
# samples under target/criterion and the harness prints source work counters;
# this command is the reproducible procedure used to refresh the report.
cargo bench --bench performance -- --sample-size 10 --measurement-time 0.1
cargo test --test performance_harness -- --nocapture
