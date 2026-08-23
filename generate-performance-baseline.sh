#!/usr/bin/env bash
set -euo pipefail

# The benchmark executable is the source of truth. Criterion writes raw
# samples under target/criterion and the harness prints source work counters;
# this command is the reproducible procedure used to refresh the report.
if [[ "${PERFORMANCE_RESULTS_ONLY:-0}" != "1" ]]; then
  cargo build --offline --release
  cargo bench --offline --bench performance -- --sample-size 10 --measurement-time 0.1 --warm-up-time 0.1 | tee target/performance-benchmark-output.txt
  cargo test --offline --test performance_harness -- --nocapture | tee target/performance-plan-output.txt
  cargo fmt --check
  cargo clippy --offline -- -D warnings
  cargo test --offline
fi

python3 - <<'PY'
import json
import re
from pathlib import Path

estimates = {}
expected = {
    "transaction_listing",
    "transaction_listing_representative",
    "transaction_listing_stress",
    "transaction_import_representative",
    "market_data_preparation_representative",
    "nav_readiness_warm_representative",
    "portfolio_retrieval_representative",
    "correlation_matrix_representative",
    "rolling_correlation_representative",
    "rolling_correlation_stress",
    "rolling_metric_representative",
    "rolling_metric_stress",
    "historical_market_data_preparation_cold",
    "historical_market_data_preparation_warm",
    "historical_market_data_preparation_partial",
    "nav_rebuild_full",
    "nav_rebuild_full_representative",
    "nav_rebuild_full_stress",
    "portfolio_retrieval_cold",
    "portfolio_retrieval_warm",
    "nav_rebuild_incremental",
    "correlation_matrix",
    "rolling_correlation",
    "startup_and_migration",
    "delayed_source_limit_1",
    "delayed_source_limit_2",
    "delayed_source_limit_4",
    "delayed_source_limit_8",
}
for expected_name in sorted(expected):
    path = Path("target/criterion/performance-baseline") / expected_name / "new/estimates.json"
    if not path.exists():
        continue
    parts = path.parts
    if len(parts) < 4:
        continue
    name = parts[-3]
    values = json.loads(path.read_text())
    sample_path = path.with_name("sample.json")
    sample_values = json.loads(sample_path.read_text()) if sample_path.exists() else {}
    samples = [
        time / iterations
        for time, iterations in zip(
            sample_values.get("times", []), sample_values.get("iters", []), strict=True
        )
    ]
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
if estimates.keys() != expected:
    missing = sorted(expected - estimates.keys())
    unexpected = sorted(estimates.keys() - expected)
    raise SystemExit(f"Criterion paths do not match harness: missing={missing}, unexpected={unexpected}")
benchmark_output = Path("target/performance-benchmark-output.txt").read_text().splitlines()
candidate_pattern = re.compile(r"delayed limit=(\d+) calls=(\d+) peak=(\d+)")
candidates = {}
rolling_work_pattern = re.compile(
    r"rolling_work_proxy label=(\w+) input=(\d+) windows=(\d+) "
    r"naive_window_value_visits=(\d+) optimized_value_updates=(\d+) "
    r"naive_window_allocations=(\d+) optimized_window_allocations=(\d+) "
    r"optimized_total_allocations=(\d+)"
)
rolling_work = {}
for line in benchmark_output:
    if match := rolling_work_pattern.search(line):
        (
            label,
            input_len,
            windows,
            naive_visits,
            optimized_updates,
            naive_allocations,
            optimized_allocations,
            optimized_total_allocations,
        ) = match.groups()
        rolling_work[label] = {
            "input": int(input_len),
            "windows": int(windows),
            "naive_window_value_visits": int(naive_visits),
            "optimized_value_updates": int(optimized_updates),
            "naive_window_allocations": int(naive_allocations),
            "optimized_window_allocations": int(optimized_allocations),
            "optimized_total_allocations": int(optimized_total_allocations),
        }
    if match := candidate_pattern.search(line):
        limit, calls, peak = map(int, match.groups())
        candidates[str(limit)] = {"calls": calls, "peak": peak}
if set(rolling_work) != {"representative", "stress"}:
    raise SystemExit(f"Rolling work proxies do not match harness: {sorted(rolling_work)}")
warm_source_calls = next(
    int(match.group(1))
    for line in benchmark_output
    if (match := re.search(r"warm preparation source_calls=(\d+)", line))
)

approved_targets = {
    "correlation_matrix": 445_531_056,
    "correlation_matrix_representative": 29_509_919_239,
    "historical_market_data_preparation_cold": 6_877_801_668,
    "historical_market_data_preparation_partial": 4_283_927_161,
    "historical_market_data_preparation_warm": 373_421_051,
    "market_data_preparation_representative": 25_443_133_881,
    "nav_readiness_warm_representative": 20_338_526,
    "nav_rebuild_full": 11_640_612_859,
    "nav_rebuild_full_representative": 6_429_392_418,
    "nav_rebuild_full_stress": 34_716_733_645,
    "nav_rebuild_incremental": 973_815_198,
    "portfolio_retrieval_cold": 13_402_702_363,
    "portfolio_retrieval_representative": 25_542_912_866,
    "portfolio_retrieval_warm": 374_044_380,
    "rolling_correlation": 141_698_194,
    "rolling_correlation_representative": 1_722_112_598,
    "rolling_metric_representative": 260_907,
    "rolling_metric_stress": 722_292,
    "startup_and_migration": 115_259_889,
    "transaction_listing": 486_397,
    "transaction_listing_representative": 21_554_013,
    "transaction_listing_stress": 98_552_625,
}
if set(approved_targets) != expected - {
    "transaction_import_representative",
    "delayed_source_limit_1",
    "delayed_source_limit_2",
    "delayed_source_limit_4",
    "delayed_source_limit_8",
    "rolling_correlation_stress",
}:
    raise SystemExit("approved performance target set does not match benchmark paths")
target_results = {
    name: {
        "observed_p95_ns": estimates[name]["p95_ns"],
        "target_p95_ns": target,
        "passed": estimates[name]["p95_ns"] <= target,
    }
    for name, target in approved_targets.items()
}
failed_targets = {
    name: (result["observed_p95_ns"], result["target_p95_ns"])
    for name, result in target_results.items()
    if not result["passed"]
}
targets = approved_targets
# The concurrency limit was approved as 4 by the PRD #19 decision gate. Keep
# rerun candidate measurements separate from that approved production contract.
approved_fixed_limit = 4
approved_startup_p95_target = 115_259_889
startup_p95 = estimates["startup_and_migration"]["p95_ns"]
report = {
    "procedure": {"samples": 10, "measurement_time_seconds": 0.1,
                  "warm_up_time_seconds": 0.1,
                  "command": "./generate-performance-baseline.sh"},
    "criterion_estimates": estimates,
    "decision_gate": {
        "path_p95_targets_ns": targets,
        "target_rule": "immutable user-approved issue #20 p95 targets",
        "fixed_target_results": target_results,
        "fixed_target_failures": failed_targets,
        "fixed_source_concurrency_limit": approved_fixed_limit,
        "approved_startup_and_migration_p95_target_ns": approved_startup_p95_target,
        "startup_and_migration_p95_target_passed": startup_p95 <= approved_startup_p95_target,
        "candidate_work": candidates,
        "warm_cache_source_calls_observed": warm_source_calls,
        "warm_cache_source_call_target": 0,
    },
    "work_and_plan_output": {
        "benchmark_stdout": "target/performance-benchmark-output.txt",
        "query_plan_stdout": "target/performance-plan-output.txt",
        "query_plan_lines": [line.strip() for line in Path("target/performance-plan-output.txt").read_text().splitlines() if "transaction_query_plans=" in line],
        "baseline_work": next(
            (
                {"source_calls": int(match.group(1)), "peak": int(match.group(2))}
                for line in benchmark_output
                if (match := re.search(r"baseline source_calls=(\d+) peak=(\d+)", line))
            ),
            None,
        ),
        "rolling_work_proxy": rolling_work,
        "query_count": {
            "approved_numeric_target": None,
            "measured_explain_statements": 5,
            "evidence": "performance_harness checks five representative SQLite query plans; no application query-count target is present in the approved baseline",
        },
    },
    "verification": {
        "commands": ["cargo fmt --check", "cargo clippy --offline -- -D warnings", "cargo test --offline"],
        "status": "failed" if failed_targets else "passed",
    },
}
Path("docs/performance-baseline-results.json").write_text(json.dumps(report, indent=2) + "\n")
if failed_targets:
    raise SystemExit(f"approved performance targets failed: {failed_targets}")
PY
