use std::path::Path;
use std::process::Command;

use sea_orm::{ActiveModelTrait, ColumnTrait, Database, EntityTrait, QueryFilter, Set};
use serde_json::{json, Value};

use rstock::db::entities::{asset, daily_asset_price, portfolio_asset_history, portfolio_history};

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
        assert_eq!(value["data"]["nav_market_data_limitations"], json!([]));
        assert_eq!(
            value["data"]["current_position_market_data_limitations"],
            json!([])
        );
        assert!(value["data"]["nav"].is_null());
        assert!(value["data"].get("nav_history").is_none());
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
    assert!(stdout.contains("unavailable"));
    assert!(stdout.contains("Performance positions value: unavailable"));
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
    portfolio_asset_history::ActiveModel {
        date: Set("9999-12-31".to_owned()),
        asset_id: Set(asset.id),
        quantity: Set(1.0),
        closing_price: Set(10.0),
        market_value: Set(10.0),
        exchange_rate: Set(1.0),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("complete NAV asset snapshot should be inserted");
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
    assert_eq!(get_stdout.lines().count(), 1);
    assert!(!get_stdout.contains("\u{1b}["));
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
    assert!(get_stdout.contains("Performance positions value: unavailable"));
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

fn run_success(home: &Path, args: &[&str]) {
    let output = command(home)
        .args(args)
        .output()
        .expect("rstock should run");
    assert!(
        output.status.success(),
        "rstock failed: {}",
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
