//! Process-level CLI tests for the Portfolio dashboard commands.
//!
//! They prove both dashboard aliases (`get` and `portfolio get`) dispatch
//! through the same library path and emit the same output document, that JSON
//! stays one compact envelope without human formatting, and that `--period`
//! feeds the requested NAV history into the JSON Adapter.

use std::path::Path;
use std::process::Command;

pub mod common;

use sea_orm::{ActiveModelTrait, ColumnTrait, Database, EntityTrait, QueryFilter, Set};
use serde_json::{json, Value};

use rstock::db::entities::{asset, daily_asset_price, portfolio_history, transaction};

#[test]
fn both_dashboard_paths_emit_the_same_empty_json_contract() {
    let cases: &[&[&str]] = &[
        &["--json", "get"],
        &["portfolio", "get", "--period", "all", "--json"],
    ];

    for args in cases {
        let home = tempfile::tempdir().expect("temporary HOME should be created");
        let output = command(home.path())
            .args(*args)
            .output()
            .expect("rstock should run");

        assert!(
            output.status.success(),
            "rstock failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value = parse_envelope(&output.stdout);
        assert_eq!(value["command"], "portfolio.get");
        let data = &value["data"];
        assert_eq!(data["base_currency"], "EUR");
        assert_eq!(
            data["inventory"]["performance_holdings"]["positions"],
            json!([])
        );
        assert_eq!(
            data["inventory"]["performance_holdings"]["aggregates"]["current_value"],
            0.0
        );
        assert_eq!(
            data["inventory"]["monetary_holdings"]["positions"],
            json!([])
        );
        assert_eq!(
            data["inventory"]["monetary_holdings"]["aggregates"]["current_value"],
            0.0
        );
        assert_eq!(data["inventory"]["total_value"], 0.0);
        assert_eq!(
            data["inventory"]["performance_holdings"]["market_data_limitations"],
            json!([])
        );
        assert_eq!(
            data["inventory"]["monetary_holdings"]["market_data_limitations"],
            json!([])
        );
        assert_eq!(data["performance"]["state"], "not_applicable");
        assert_eq!(data["nav_history"], json!([]));
    }
}

#[test]
fn empty_dashboard_keeps_human_table_and_chart_messages() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    let output = command(home.path())
        .arg("get")
        .output()
        .expect("rstock should run");

    assert!(
        output.status.success(),
        "rstock failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("No positions found."));
    assert!(stdout.contains("Performance: not applicable"));
    assert!(stdout.contains("Not enough data to display NAV chart."));
    assert!(!stdout.trim_start().starts_with('{'));
}

#[tokio::test]
async fn dashboard_keeps_weight_and_marks_unavailable_values_in_human_output() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    insert_ledger_with_offline_nav(home.path()).await;

    let output = command(home.path())
        .arg("get")
        .output()
        .expect("rstock should run");
    assert!(
        output.status.success(),
        "rstock failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("Weight"));
    // The unpriced position keeps its ledger facts while price-dependent
    // facts and dependent aggregates render as unavailable.
    assert!(stdout.contains("Value: unavailable"));
    assert!(stdout.contains("Total value: unavailable"));
    // The synchronized performance facts stay separately labeled with their
    // own Effective valuation date.
    assert!(stdout.contains("Performance value:"));
    assert!(stdout.contains("NAV:"));
}

async fn insert_offline_nav_fixture(home: &Path) {
    let db = Database::connect(format!(
        "sqlite:{}?mode=rwc",
        home.join(".rstock/rstock.db").display()
    ))
    .await
    .expect("CLI database should be available");
    let asset = asset::Entity::find()
        .filter(asset::Column::Ticker.eq("IE00XFAKE001"))
        .one(&db)
        .await
        .expect("asset lookup should succeed")
        .expect("test asset should exist");
    daily_asset_price::ActiveModel {
        id: sea_orm::ActiveValue::NotSet,
        asset_id: Set(asset.id),
        date: Set("9999-12-30".to_owned()),
        closing_price: Set(10.0),
        is_api_failure: Set(false),
    }
    .insert(&db)
    .await
    .expect("out-of-window cached price should be inserted");
    portfolio_history::ActiveModel {
        date: Set("9999-12-31".to_owned()),
        asset_value: Set(0.0),
        total_value: Set(0.0),
        outstanding_shares: Set(0.0),
        nav: Set(100.0),
    }
    .insert(&db)
    .await
    .expect("NAV readiness snapshot should be inserted");
}

#[tokio::test]
async fn dashboard_aliases_emit_identical_json_for_the_same_ledger() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    insert_ledger_with_offline_nav(home.path()).await;

    let get_output = command(home.path())
        .args(["--json", "get"])
        .output()
        .expect("rstock should run");
    let alias_output = command(home.path())
        .args(["portfolio", "get", "--json"])
        .output()
        .expect("rstock should run");

    assert!(
        get_output.status.success(),
        "get failed: {}",
        String::from_utf8_lossy(&get_output.stderr)
    );
    assert!(
        alias_output.status.success(),
        "portfolio get failed: {}",
        String::from_utf8_lossy(&alias_output.stderr)
    );
    let get_stdout = String::from_utf8(get_output.stdout).expect("stdout should be UTF-8");
    let alias_stdout = String::from_utf8(alias_output.stdout).expect("stdout should be UTF-8");
    assert_eq!(
        get_stdout, alias_stdout,
        "dashboard aliases must dispatch identically"
    );
    // JSON remains one compact envelope with raw values only.
    assert_eq!(get_stdout.lines().count(), 1);
    assert!(!get_stdout.contains("\u{1b}["));
    let value: Value = serde_json::from_str(&get_stdout).expect("stdout should be one JSON value");
    assert_eq!(value["data"]["performance"]["state"], "available");
    assert_eq!(
        value["data"]["performance"]["nav_history_limitations"],
        json!([])
    );
    assert!(
        value["data"]["inventory"]["performance_holdings"]["positions"][0]["current_value"]
            .is_null()
    );
    assert_eq!(value["data"]["nav_history"][0]["date"], "9999-12-31");
    assert_eq!(value["data"]["nav_history"][0]["nav"], 100.0);
}

#[tokio::test]
async fn dashboard_aliases_emit_identical_human_output_for_the_same_ledger() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    insert_ledger_with_offline_nav(home.path()).await;

    let get_output = command(home.path())
        .arg("get")
        .output()
        .expect("rstock should run");
    let alias_output = command(home.path())
        .args(["portfolio", "get"])
        .output()
        .expect("rstock should run");

    assert!(
        get_output.status.success(),
        "get failed: {}",
        String::from_utf8_lossy(&get_output.stderr)
    );
    assert!(
        alias_output.status.success(),
        "portfolio get failed: {}",
        String::from_utf8_lossy(&alias_output.stderr)
    );
    let get_stdout = String::from_utf8(get_output.stdout).expect("stdout should be UTF-8");
    let alias_stdout = String::from_utf8(alias_output.stdout).expect("stdout should be UTF-8");
    assert_eq!(
        get_stdout, alias_stdout,
        "dashboard aliases must dispatch identically"
    );
    assert!(get_stdout.contains("Weight"));
    assert!(get_stdout.contains("unavailable"));
    // No JSON envelope leaks into human output.
    assert!(!get_stdout.trim_start().starts_with('{'));
}

/// `--period` reaches both output Adapters through the composed outcome: the
/// requested ready NAV history shrinks for short periods and covers the whole
/// measurable lifetime for `all`.
#[tokio::test]
async fn period_selects_the_requested_ready_nav_history_in_both_formats() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    insert_ledger_with_2025_nav_history(home.path()).await;

    let all_json = command(home.path())
        .args(["portfolio", "get", "--period", "all", "--json"])
        .output()
        .expect("rstock should run");
    let one_month_json = command(home.path())
        .args(["get", "--period", "1m", "--json"])
        .output()
        .expect("rstock should run");
    let all_human = command(home.path())
        .args(["portfolio", "get", "--period", "all"])
        .output()
        .expect("rstock should run");
    let one_month_human = command(home.path())
        .args(["get", "--period", "1m"])
        .output()
        .expect("rstock should run");

    assert!(
        all_json.status.success(),
        "portfolio get failed: {}",
        String::from_utf8_lossy(&all_json.stderr)
    );
    assert!(
        one_month_json.status.success(),
        "get failed: {}",
        String::from_utf8_lossy(&one_month_json.stderr)
    );
    assert!(
        all_human.status.success(),
        "portfolio get failed: {}",
        String::from_utf8_lossy(&all_human.stderr)
    );
    assert!(
        one_month_human.status.success(),
        "get failed: {}",
        String::from_utf8_lossy(&one_month_human.stderr)
    );

    let all_value = parse_envelope(&all_json.stdout);
    assert_eq!(all_value["data"]["performance"]["state"], "available");
    let all_history = all_value["data"]["nav_history"]
        .as_array()
        .expect("nav_history must be an array");
    assert!(
        all_history.len() >= 5,
        "the all period covers the rebuilt NAV history: {all_history:?}"
    );
    assert_eq!(
        all_history.last().expect("history should have an end")["date"],
        "2025-06-09"
    );

    // The one-month window starts after the ready history ends in 2025, so it
    // yields no history: the period controls the JSON output. This holds for
    // any run date after July 2025, which the seed's fixed history guarantees.
    let short_value = parse_envelope(&one_month_json.stdout);
    assert_eq!(short_value["data"]["nav_history"], json!([]));

    // The same period selection drives the human chart labels.
    let all_human_text = String::from_utf8(all_human.stdout).expect("stdout should be UTF-8");
    assert!(all_human_text.contains("NAV — All"), "{all_human_text}");
    let one_month_human_text =
        String::from_utf8(one_month_human.stdout).expect("stdout should be UTF-8");
    assert!(
        one_month_human_text.contains("Not enough data to display NAV chart."),
        "{one_month_human_text}"
    );
}

#[test]
fn failing_command_exits_nonzero_through_the_library_dispatch() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    let output = command(home.path())
        .args(["transaction", "delete", "999", "--yes"])
        .output()
        .expect("rstock should run");

    assert!(
        !output.status.success(),
        "deleting a missing transaction must fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("not found"), "stderr was: {stderr}");
}

fn parse_envelope(stdout: &[u8]) -> Value {
    common::parse_json_envelope(stdout)
}

async fn insert_ledger_with_offline_nav(home: &Path) {
    run_success(
        home,
        &[
            "portfolio",
            "asset",
            "add",
            "-t",
            "IE00XFAKE001",
            "-n",
            "Unavailable asset",
            "-T",
            "fund",
            "--asset-class",
            "equity",
            "--morningstar-code",
            "F00000XFAKE",
        ],
    );
    run_success(
        home,
        &[
            "transaction",
            "buy",
            "-t",
            "IE00XFAKE001",
            "-d",
            "01-01-2020",
            "-q",
            "1",
            "-p",
            "10",
        ],
    );
    insert_offline_nav_fixture(home).await;
}

/// A ledger whose complete NAV rebuild needs no market-data source call: the
/// cached daily prices cover the whole positive-holding interval, so readiness
/// builds a 2025 NAV history through 09-06-2025 and stops there with a
/// reported staleness limitation.
async fn insert_ledger_with_2025_nav_history(home: &Path) {
    run_success(
        home,
        &[
            "portfolio",
            "asset",
            "add",
            "-t",
            "IE00XFAKE100",
            "-n",
            "History Fund",
            "-T",
            "fund",
            "--asset-class",
            "equity",
            "--morningstar-code",
            "F00000XFA100",
        ],
    );
    run_success(
        home,
        &[
            "transaction",
            "buy",
            "-t",
            "IE00XFAKE100",
            "-d",
            "02-06-2025",
            "-q",
            "2",
            "-p",
            "100",
        ],
    );

    let db = Database::connect(format!(
        "sqlite:{}?mode=rwc",
        home.join(".rstock/rstock.db").display()
    ))
    .await
    .expect("CLI database should be available");
    let asset_record = asset::Entity::find()
        .filter(asset::Column::Ticker.eq("IE00XFAKE100"))
        .one(&db)
        .await
        .expect("asset lookup should succeed")
        .expect("test asset should exist");
    let transaction_record = transaction::Entity::find()
        .filter(transaction::Column::AssetId.eq(asset_record.id))
        .one(&db)
        .await
        .expect("transaction lookup should succeed")
        .expect("recorded buy should exist");
    assert_eq!(
        transaction_record.date, "2025-06-02",
        "the CLI stores DD-MM-YYYY input in YYYY-MM-DD form"
    );

    // Contiguous calendar-day closing prices so the cached coverage needs no
    // source request for the positive-holding interval; readiness then builds
    // NAV through 09-06-2025 and reports the later staleness as a limitation.
    for (index, date) in (0..8)
        .map(|offset| {
            chrono::NaiveDate::from_ymd_opt(2025, 6, 2).expect("fixed date")
                + chrono::Duration::days(offset)
        })
        .enumerate()
    {
        daily_asset_price::ActiveModel {
            asset_id: Set(asset_record.id),
            date: Set(date.format("%Y-%m-%d").to_string()),
            closing_price: Set(100.0 + index as f64),
            is_api_failure: Set(false),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("cached weekday price should be inserted");
    }
}

fn run_success(home: &Path, args: &[&str]) {
    let output = command(home)
        .args(args)
        .output()
        .expect("rstock should run");
    assert!(
        output.status.success(),
        "rstock failed: {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn command(home: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_rstock"));
    command
        .env("HOME", home)
        .env("RSTOCK_SOURCE_TOKEN_PAGE_URL", "file:///nonexistent/token")
        .env(
            "RSTOCK_SOURCE_CHARTSERVICE_URL",
            "file:///nonexistent/chart",
        )
        .env("RSTOCK_SOURCE_HOLDINGS_URL", "file:///nonexistent/holdings")
        .env("RSTOCK_SOURCE_QUOTE_URL", "file:///nonexistent/quote")
        .env("RSTOCK_SOURCE_SAL_API_KEY", "test")
        .env("RSTOCK_SOURCE_USER_AGENT", "rstock-test")
        .env(
            "RSTOCK_SOURCE_TOKEN_CACHE_PATH",
            home.join("token-cache.json"),
        );
    command
}
