use std::process::Command;

use serde_json::{json, Value};

#[test]
fn both_dashboard_paths_emit_the_same_empty_json_contract() {
    let cases: &[&[&str]] = &[
        &["--json", "get"],
        &["portfolio", "get", "--period", "all", "--json"],
    ];

    for args in cases {
        let home = tempfile::tempdir().expect("temporary HOME should be created");
        let output = Command::new(env!("CARGO_BIN_EXE_rstock"))
            .args(*args)
            .env("HOME", home.path())
            .output()
            .expect("rstock should run");

        assert!(
            output.status.success(),
            "rstock failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
        assert_eq!(stdout.lines().count(), 1);
        assert!(!stdout.contains("\u{1b}["));
        let value: Value = serde_json::from_str(&stdout).expect("stdout should be one JSON value");
        assert_eq!(value["command"], "portfolio.get");
        assert_eq!(value["data"]["base_currency"], "EUR");
        assert_eq!(value["data"]["positions"], json!([]));
        assert_eq!(value["data"]["monetary_positions"], json!([]));
        assert_eq!(value["data"]["total_monetary_value"], 0.0);
        assert_eq!(value["data"]["monetary_market_data_limitations"], json!([]));
        assert_eq!(value["data"]["market_data_limitations"], json!([]));
        assert!(value["data"]["nav"].is_null());
        assert!(value["data"].get("nav_history").is_none());
    }
}

#[test]
fn empty_dashboard_keeps_human_table_and_chart_messages() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    let output = Command::new(env!("CARGO_BIN_EXE_rstock"))
        .arg("get")
        .env("HOME", home.path())
        .output()
        .expect("rstock should run");

    assert!(
        output.status.success(),
        "rstock failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("No positions found."));
    assert!(stdout.contains("Not enough data to display NAV chart."));
    assert!(!stdout.trim_start().starts_with('{'));
}
