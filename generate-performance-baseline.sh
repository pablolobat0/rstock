#!/usr/bin/env bash
set -euo pipefail

# The benchmark executable is the source of truth. Criterion writes raw
# samples under target/criterion and the harness prints source work counters;
# this command is the reproducible procedure used to refresh the report.
cargo bench --bench performance -- --sample-size 10 --measurement-time 0.1 | tee target/performance-benchmark-output.txt
cargo test --test performance_harness -- --nocapture | tee target/performance-plan-output.txt

python3 - <<'PY'
import json
from pathlib import Path

estimates = {}
for path in Path("target/criterion").glob("**/new/estimates.json"):
    parts = path.parts
    if len(parts) < 4:
        continue
    name = ".".join(parts[-3:-1])
    values = json.loads(path.read_text())
    sample_path = path.with_name("sample.json")
    sample_values = json.loads(sample_path.read_text()) if sample_path.exists() else {}
    samples = sample_values.get("times", [])
    if samples:
        ordered = sorted(samples)
        p95 = ordered[min(len(ordered) - 1, int(len(ordered) * 0.95))]
    else:
        p95 = None
    estimates[name] = {
        "mean_ns": values["mean"]["point_estimate"],
        "median_ns": values["median"]["point_estimate"],
        "std_dev_ns": values["std_dev"]["point_estimate"],
        "p95_ns": p95,
    }
report = {
    "procedure": {"samples": 10, "measurement_time_seconds": 0.1,
                  "command": "./generate-performance-baseline.sh"},
    "criterion_estimates": estimates,
    "work_and_plan_output": {
        "benchmark_stdout": "target/performance-benchmark-output.txt",
        "query_plan_stdout": "target/performance-plan-output.txt",
        "query_plan_lines": [line.strip() for line in Path("target/performance-plan-output.txt").read_text().splitlines() if "transaction_query_plans=" in line],
        "work_count_lines": [line.strip() for line in Path("target/performance-benchmark-output.txt").read_text().splitlines() if "baseline source_calls=" in line],
    },
}
Path("docs/performance-baseline-results.json").write_text(json.dumps(report, indent=2) + "\n")
PY
