#!/usr/bin/env bash
set -euo pipefail

if [[ "${PERFORMANCE_RESULTS_ONLY:-0}" != "1" ]]; then
  cargo build --offline --release
  cargo bench --offline --bench performance startup_ -- --sample-size 20 --measurement-time 2 --warm-up-time 0.2
  cargo test --offline --test performance_harness automatic_migration_covers_fresh_partial_and_current_schemas
fi

python3 - <<'PY'
import json
from pathlib import Path

names = [
    "startup_and_migration_transactional",
    "startup_executable_only",
    "startup_logging_cold",
    "startup_logging_warm",
    "startup_database_connection_cold",
    "startup_database_connection_warm",
    "startup_automatic_migration_cold",
    "startup_automatic_migration_warm",
    "startup_automatic_migration_warm_unbatched",
    "startup_transaction_list_cold",
    "startup_transaction_list_warm",
]

estimates = {}
root = Path("target/criterion/performance-baseline")
for name in names:
    estimate_path = root / name / "new/estimates.json"
    sample_path = root / name / "new/sample.json"
    if not estimate_path.exists() or not sample_path.exists():
        raise SystemExit(f"missing Criterion result for {name}; run this script without PERFORMANCE_RESULTS_ONLY")
    values = json.loads(estimate_path.read_text())
    sample_values = json.loads(sample_path.read_text())
    samples = sorted(
        time / iterations
        for time, iterations in zip(
            sample_values["times"], sample_values["iters"], strict=True
        )
    )
    estimates[name] = {
        "mean_ns": values["mean"]["point_estimate"],
        "median_ns": values["median"]["point_estimate"],
        "std_dev_ns": values["std_dev"]["point_estimate"],
        "p95_ns": samples[min(len(samples) - 1, int(len(samples) * 0.95))],
    }

report = {
    "procedure": {
        "samples": 20,
        "measurement_time_seconds": 2,
        "warm_up_time_seconds": 0.2,
        "command": "./generate-startup-performance-results.sh",
    },
    "criterion_estimates": estimates,
    "acceptance": {
        "approved_issue_20_targets": {},
        "fixed_target_results": {},
        "all_paths_informational": True,
    },
    "conclusion": "Startup-specific paths were added after the issue #20 decision gate and remain informational evidence; immutable issue #20 target enforcement is reported by generate-performance-baseline.sh.",
}
Path("docs/startup-performance-results.json").write_text(json.dumps(report, indent=2) + "\n")
PY
