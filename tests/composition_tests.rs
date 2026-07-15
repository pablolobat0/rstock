mod common;

use std::process::Command;

use rstock::models::StockInfo;
use rstock::services::composition::compute_composition;
use sea_orm::{EntityTrait, Set};
use serde_json::{json, Value};

use common::{
    insert_daily_price, insert_portfolio_snapshot, insert_transaction, setup_test_db,
    MockMarketDataSources,
};
use rstock::db::entities::{asset, portfolio_asset_history};

async fn insert_classified_asset(
    db: &sea_orm::DatabaseConnection,
    ticker: &str,
    name: &str,
    asset_type: &str,
    currency: &str,
    asset_class: Option<&str>,
    equity_style: Option<&str>,
    management: Option<&str>,
) -> i32 {
    let record = asset::ActiveModel {
        ticker: Set(ticker.to_owned()),
        name: Set(name.to_owned()),
        asset_type: Set(asset_type.to_owned()),
        currency: Set(currency.to_owned()),
        created_at: Set("2025-01-01T00:00:00".to_owned()),
        asset_class: Set(asset_class.map(str::to_owned)),
        equity_style: Set(equity_style.map(str::to_owned)),
        management: Set(management.map(str::to_owned)),
        ..Default::default()
    };
    let result = asset::Entity::insert(record)
        .exec(db)
        .await
        .expect("failed to insert classified asset");
    result.last_insert_id
}

async fn insert_asset_snapshot(
    db: &sea_orm::DatabaseConnection,
    date: &str,
    asset_id: i32,
    quantity: f64,
    closing_price: f64,
    market_value: f64,
) {
    let record = portfolio_asset_history::ActiveModel {
        date: Set(date.to_owned()),
        asset_id: Set(asset_id),
        quantity: Set(quantity),
        closing_price: Set(closing_price),
        market_value: Set(market_value),
        exchange_rate: Set(1.0),
        ..Default::default()
    };
    portfolio_asset_history::Entity::insert(record)
        .exec(db)
        .await
        .expect("failed to insert asset snapshot");
}

fn mock_stock_info(
    ticker: &str,
    sector: Option<&str>,
    industry: Option<&str>,
    country: Option<&str>,
    market_cap: Option<f64>,
) -> StockInfo {
    StockInfo {
        ticker: ticker.to_owned(),
        name: Some(format!("{ticker} Inc")),
        currency: Some("USD".to_owned()),
        current_price: Some(100.0),
        previous_close: Some(99.0),
        day_range: None,
        fifty_two_week_range: None,
        volume: None,
        avg_volume: None,
        market_cap,
        pe_ttm: None,
        eps_ttm: None,
        dividend_yield: None,
        sector: sector.map(str::to_owned),
        industry: industry.map(str::to_owned),
        country: country.map(str::to_owned),
    }
}

fn composition_envelope(result: &rstock::models::CompositionResult) -> Value {
    let mut output = Vec::new();
    rstock::cli::output::write_json(&mut output, "analyze.composition", result)
        .expect("composition JSON should serialize");
    let text = String::from_utf8(output).expect("composition JSON should be UTF-8");
    assert_eq!(text.lines().count(), 1);
    serde_json::from_str(&text).expect("composition output should be valid JSON")
}

#[tokio::test]
async fn test_composition_direct_stocks_only() {
    let db = setup_test_db().await;

    let id1 = insert_classified_asset(
        &db,
        "XFAKE1",
        "TechCo",
        "stock",
        "EUR",
        Some("equity"),
        Some("growth"),
        Some("passive"),
    )
    .await;
    let id2 = insert_classified_asset(
        &db,
        "XFAKE2",
        "HealthCo",
        "stock",
        "EUR",
        Some("equity"),
        Some("value"),
        Some("active"),
    )
    .await;

    // Insert transactions
    insert_transaction(&db, id1, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_transaction(&db, id2, "2025-01-02", 5.0, 200.0, 0.0).await;

    // Insert prices
    insert_daily_price(&db, id1, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, id2, "2025-01-02", 200.0, false).await;

    // Insert portfolio snapshot (needed for get_asset_positions)
    insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 20.0).await;
    insert_asset_snapshot(&db, "2025-01-02", id1, 10.0, 100.0, 1000.0).await;
    insert_asset_snapshot(&db, "2025-01-02", id2, 5.0, 200.0, 1000.0).await;

    let mut fetcher = MockMarketDataSources::new();
    fetcher
        .historical_prices
        .insert("XFAKE1".to_owned(), vec![("2025-01-02".to_owned(), 100.0)]);
    fetcher
        .historical_prices
        .insert("XFAKE2".to_owned(), vec![("2025-01-02".to_owned(), 200.0)]);
    fetcher.stock_info.insert(
        "XFAKE1".to_owned(),
        mock_stock_info(
            "XFAKE1",
            Some("Technology"),
            Some("Semiconductors"),
            Some("United States"),
            Some(50_000_000_000.0),
        ),
    );
    fetcher.stock_info.insert(
        "XFAKE2".to_owned(),
        mock_stock_info(
            "XFAKE2",
            Some("Healthcare"),
            Some("Pharma"),
            Some("Germany"),
            Some(5_000_000_000.0),
        ),
    );

    let result = compute_composition(&db, &common::market_data(&fetcher))
        .await
        .unwrap();

    // Asset class breakdown: both are equity
    assert_eq!(result.asset_class_breakdown.len(), 1);
    assert_eq!(result.asset_class_breakdown[0].label, "equity");

    // Sector breakdown: Technology and Healthcare
    assert_eq!(result.sector_breakdown.len(), 2);

    // Country breakdown: US and Germany
    assert_eq!(result.country_breakdown.len(), 2);

    // Market cap: one large (50B), one mid (5B)
    assert_eq!(result.market_cap_breakdown.len(), 2);

    let envelope = composition_envelope(&result);
    assert_eq!(envelope["command"], "analyze.composition");
    let data = &envelope["data"];
    for field in [
        "asset_class_breakdown",
        "equity_style_breakdown",
        "management_breakdown",
        "sector_breakdown",
        "country_breakdown",
        "market_cap_breakdown",
        "top_holdings",
        "warnings",
    ] {
        assert!(data[field].is_array(), "{field} should be an array");
    }
    assert_eq!(data["equity_style_breakdown"].as_array().unwrap().len(), 2);
    assert_eq!(data["management_breakdown"].as_array().unwrap().len(), 2);
    assert_eq!(data["top_holdings"].as_array().unwrap().len(), 2);
    assert!(data["top_holdings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|holding| holding["ticker"] == "XFAKE1"));
}

#[tokio::test]
async fn test_composition_unclassified_asset() {
    let db = setup_test_db().await;

    let id1 =
        insert_classified_asset(&db, "XFAKE1", "UnknownCo", "stock", "EUR", None, None, None).await;

    insert_transaction(&db, id1, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, id1, "2025-01-02", 100.0, false).await;
    insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 10.0).await;
    insert_asset_snapshot(&db, "2025-01-02", id1, 10.0, 100.0, 1000.0).await;

    let mut fetcher = MockMarketDataSources::new();
    fetcher
        .historical_prices
        .insert("XFAKE1".to_owned(), vec![("2025-01-02".to_owned(), 100.0)]);
    fetcher.stock_info.insert(
        "XFAKE1".to_owned(),
        mock_stock_info("XFAKE1", Some("Technology"), None, Some("US"), None),
    );

    let result = compute_composition(&db, &common::market_data(&fetcher))
        .await
        .unwrap();

    // Unclassified asset
    assert_eq!(result.asset_class_breakdown.len(), 1);
    assert_eq!(result.asset_class_breakdown[0].label, "Unclassified");
}

#[tokio::test]
async fn test_composition_empty_portfolio() {
    let db = setup_test_db().await;
    let fetcher = MockMarketDataSources::new();

    let result = compute_composition(&db, &common::market_data(&fetcher))
        .await
        .unwrap();

    assert!(result.asset_class_breakdown.is_empty());
    assert!(!result.warnings.is_empty());

    let envelope = composition_envelope(&result);
    let data = &envelope["data"];
    for field in [
        "asset_class_breakdown",
        "equity_style_breakdown",
        "management_breakdown",
        "sector_breakdown",
        "country_breakdown",
        "market_cap_breakdown",
        "top_holdings",
    ] {
        assert_eq!(data[field], json!([]), "{field} should remain empty");
    }
    assert_eq!(data["warnings"], json!(["Portfolio has no value."]));
}

#[tokio::test]
async fn test_composition_failed_stock_info() {
    let db = setup_test_db().await;

    let id1 = insert_classified_asset(
        &db,
        "XFAKE1",
        "FailCo",
        "stock",
        "EUR",
        Some("equity"),
        None,
        None,
    )
    .await;

    insert_transaction(&db, id1, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, id1, "2025-01-02", 100.0, false).await;
    insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 10.0).await;
    insert_asset_snapshot(&db, "2025-01-02", id1, 10.0, 100.0, 1000.0).await;

    let mut fetcher = MockMarketDataSources::new();
    fetcher
        .historical_prices
        .insert("XFAKE1".to_owned(), vec![("2025-01-02".to_owned(), 100.0)]);
    // No stock_info for XFAKE1 -> get_stock_info will fail

    let result = compute_composition(&db, &common::market_data(&fetcher))
        .await
        .unwrap();

    // Should have a warning about failed lookup
    assert!(result.warnings.iter().any(|w| w.contains("XFAKE1")));

    let envelope = composition_envelope(&result);
    let data = &envelope["data"];
    assert!(data["warnings"][0]
        .as_str()
        .is_some_and(|warning| warning.contains("XFAKE1")));
    assert!(data["top_holdings"][0]["country"].is_null());
    assert!(data["top_holdings"][0]["sector"].is_null());
}

#[test]
fn composition_command_emits_one_json_envelope() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    let output = Command::new(env!("CARGO_BIN_EXE_rstock"))
        .args(["analyze", "composition", "--json"])
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
    let envelope: Value = serde_json::from_str(&stdout).expect("stdout should be valid JSON");
    assert_eq!(envelope["command"], "analyze.composition");
    assert_eq!(envelope["data"]["top_holdings"], json!([]));
    assert_eq!(
        envelope["data"]["warnings"],
        json!(["Portfolio has no value."])
    );
}
