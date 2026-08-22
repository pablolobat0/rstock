mod common;

use std::path::Path;
use std::process::Command;

use rstock::models::{FundData, FundHolding, StockInfo};
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
    _industry: Option<&str>,
    country: Option<&str>,
    market_cap: Option<f64>,
) -> StockInfo {
    StockInfo {
        name: Some(format!("{ticker} Inc")),
        market_cap,
        sector: sector.map(str::to_owned),
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

    // Insert historical snapshots for the asset-series setup.
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
    let asset_class_breakdown = result.asset_class_breakdown.as_ref().unwrap();
    assert_eq!(asset_class_breakdown.len(), 1);
    assert_eq!(asset_class_breakdown[0].label, "equity");

    // Sector breakdown: Technology and Healthcare
    assert_eq!(result.sector_breakdown.as_ref().unwrap().len(), 2);

    // Country breakdown: US and Germany
    assert_eq!(result.country_breakdown.as_ref().unwrap().len(), 2);

    // Market cap: one large (50B), one mid (5B)
    assert_eq!(result.market_cap_breakdown.as_ref().unwrap().len(), 2);

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
        "market_data_limitations",
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
async fn complete_composition_preserves_fund_look_through() {
    let db = setup_test_db().await;
    let fund = asset::ActiveModel {
        ticker: Set("XFAKEFUND".to_owned()),
        name: Set("Equity Fund".to_owned()),
        asset_type: Set("fund".to_owned()),
        currency: Set("EUR".to_owned()),
        created_at: Set("2025-01-01T00:00:00".to_owned()),
        asset_class: Set(Some("equity".to_owned())),
        management: Set(Some("passive".to_owned())),
        morningstar_code: Set(Some("F000FAKE".to_owned())),
        ..Default::default()
    };
    let fund_id = asset::Entity::insert(fund)
        .exec(&db)
        .await
        .unwrap()
        .last_insert_id;
    insert_transaction(&db, fund_id, "2025-06-01", 1.0, 100.0, 0.0).await;
    insert_daily_price(&db, fund_id, "2025-06-04", 100.0, false).await;

    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert(
        "F000FAKE".to_owned(),
        FundData {
            fund_currency: Some("EUR".to_owned()),
            total_holdings: Some(2),
            portfolio_date: Some("2025-05-31".to_owned()),
            holdings: vec![
                FundHolding {
                    name: "Underlying One".to_owned(),
                    weighting: 60.0,
                    ticker: Some("XUNDER1".to_owned()),
                    sector: Some("Technology".to_owned()),
                    country: Some("US".to_owned()),
                    currency: Some("USD".to_owned()),
                },
                FundHolding {
                    name: "Underlying Two".to_owned(),
                    weighting: 40.0,
                    ticker: Some("XUNDER2".to_owned()),
                    sector: Some("Healthcare".to_owned()),
                    country: Some("DE".to_owned()),
                    currency: Some("EUR".to_owned()),
                },
            ],
        },
    );
    let market_data = common::market_data_at(
        &sources,
        chrono::NaiveDate::from_ymd_opt(2025, 6, 5).unwrap(),
    );

    let result = compute_composition(&db, &market_data).await.unwrap();

    let sectors = result.sector_breakdown.as_ref().unwrap();
    assert_eq!(sectors.len(), 2);
    assert!(sectors
        .iter()
        .any(|entry| entry.label == "Technology" && (entry.weight - 60.0).abs() < 1e-9));
    let holdings = result.top_holdings.as_ref().unwrap();
    assert_eq!(holdings.len(), 2);
    assert_eq!(holdings[0].ticker.as_deref(), Some("XUNDER1"));
    assert!((holdings[0].weight - 60.0).abs() < 1e-9);
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
    let breakdown = result.asset_class_breakdown.as_ref().unwrap();
    assert_eq!(breakdown.len(), 1);
    assert_eq!(breakdown[0].label, "Unclassified");
}

#[tokio::test]
async fn test_composition_empty_portfolio() {
    let db = setup_test_db().await;
    let fetcher = MockMarketDataSources::new();

    let result = compute_composition(&db, &common::market_data(&fetcher))
        .await
        .unwrap();

    assert!(result.asset_class_breakdown.as_ref().unwrap().is_empty());
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
async fn composition_uses_current_post_snapshot_inventory() {
    let db = setup_test_db().await;
    let old_id = insert_classified_asset(
        &db,
        "XFAKEOLD",
        "Existing Equity",
        "stock",
        "EUR",
        Some("equity"),
        Some("growth"),
        Some("active"),
    )
    .await;
    let new_id = insert_classified_asset(
        &db,
        "XFAKENEW",
        "New Bond",
        "stock",
        "EUR",
        Some("fixed income"),
        None,
        Some("passive"),
    )
    .await;
    insert_transaction(&db, old_id, "2025-06-01", 1.0, 100.0, 0.0).await;
    insert_transaction(&db, new_id, "2025-06-03", 1.0, 100.0, 0.0).await;
    insert_daily_price(&db, old_id, "2025-06-04", 100.0, false).await;
    insert_daily_price(&db, new_id, "2025-06-04", 100.0, false).await;
    insert_portfolio_snapshot(&db, "2025-06-02", 100.0, 1.0).await;
    insert_asset_snapshot(&db, "2025-06-02", old_id, 1.0, 100.0, 100.0).await;

    let mut sources = MockMarketDataSources::new();
    sources.stock_info.insert(
        "XFAKEOLD".to_owned(),
        mock_stock_info("XFAKEOLD", Some("Technology"), None, Some("US"), None),
    );
    sources.stock_info.insert(
        "XFAKENEW".to_owned(),
        mock_stock_info("XFAKENEW", None, None, Some("US"), None),
    );
    let market_data = common::market_data_at(
        &sources,
        chrono::NaiveDate::from_ymd_opt(2025, 6, 5).unwrap(),
    );

    let result = compute_composition(&db, &market_data).await.unwrap();

    let breakdown = result.asset_class_breakdown.as_ref().unwrap();
    assert_eq!(breakdown.len(), 2);
    assert!(breakdown.iter().any(|entry| entry.label == "equity"));
    assert!(breakdown.iter().any(|entry| entry.label == "fixed income"));
    assert_eq!(
        rstock::db::repos::portfolio_history_repo::find_latest(&db)
            .await
            .unwrap()
            .unwrap()
            .date,
        "2025-06-02"
    );
}

#[tokio::test]
async fn unavailable_position_makes_all_value_dependent_composition_unavailable() {
    let db = setup_test_db().await;
    let priced_id = insert_classified_asset(
        &db,
        "XFAKEPRICED",
        "Priced Equity",
        "stock",
        "EUR",
        Some("equity"),
        Some("growth"),
        Some("active"),
    )
    .await;
    let unpriced_id = insert_classified_asset(
        &db,
        "XFAKEUNPRICED",
        "Unpriced Equity",
        "stock",
        "EUR",
        Some("equity"),
        Some("value"),
        Some("passive"),
    )
    .await;
    insert_transaction(&db, priced_id, "2025-06-01", 1.0, 100.0, 0.0).await;
    insert_transaction(&db, unpriced_id, "2025-06-01", 1.0, 100.0, 0.0).await;
    insert_daily_price(&db, priced_id, "2025-06-04", 100.0, false).await;
    let market_data = common::market_data_at(
        &MockMarketDataSources::new(),
        chrono::NaiveDate::from_ymd_opt(2025, 6, 5).unwrap(),
    );

    let result = compute_composition(&db, &market_data).await.unwrap();

    assert!(result.asset_class_breakdown.is_none());
    assert!(result.equity_style_breakdown.is_none());
    assert!(result.management_breakdown.is_none());
    assert!(result.sector_breakdown.is_none());
    assert!(result.country_breakdown.is_none());
    assert!(result.market_cap_breakdown.is_none());
    assert!(result.top_holdings.is_none());
    assert_eq!(result.market_data_limitations.len(), 1);
    assert!(result.warnings[0].contains("current performance position"));

    let envelope = composition_envelope(&result);
    let data = &envelope["data"];
    assert!(data["asset_class_breakdown"].is_null());
    assert!(data["top_holdings"].is_null());
    assert_eq!(
        data["market_data_limitations"][0]["subject"]["ticker"],
        "XFAKEUNPRICED"
    );
    assert!(data.get("nav_market_data_limitations").is_none());
}

#[tokio::test]
async fn composition_excludes_monetary_holdings_from_weights() {
    let db = setup_test_db().await;
    let stock_id = insert_classified_asset(
        &db,
        "XFAKEPERF",
        "Performance Equity",
        "stock",
        "EUR",
        Some("equity"),
        Some("growth"),
        Some("active"),
    )
    .await;
    let monetary_id =
        common::insert_monetary_fund_asset(&db, "XFAKEMONEY", "Monetary Fund", "EUR", "F000MONEY")
            .await;
    insert_transaction(&db, stock_id, "2025-06-01", 1.0, 100.0, 0.0).await;
    insert_transaction(&db, monetary_id, "2025-06-01", 100.0, 100.0, 0.0).await;
    insert_daily_price(&db, stock_id, "2025-06-04", 100.0, false).await;
    insert_daily_price(&db, monetary_id, "2025-06-04", 100.0, false).await;
    let mut sources = MockMarketDataSources::new();
    sources.stock_info.insert(
        "XFAKEPERF".to_owned(),
        mock_stock_info("XFAKEPERF", Some("Technology"), None, Some("US"), None),
    );
    let market_data = common::market_data_at(
        &sources,
        chrono::NaiveDate::from_ymd_opt(2025, 6, 5).unwrap(),
    );

    let result = compute_composition(&db, &market_data).await.unwrap();

    let breakdown = result.asset_class_breakdown.as_ref().unwrap();
    assert_eq!(breakdown.len(), 1);
    assert_eq!(breakdown[0].label, "equity");
    assert!((breakdown[0].weight - 100.0).abs() < 1e-9);
    let top_holdings = result.top_holdings.as_ref().unwrap();
    assert_eq!(top_holdings.len(), 1);
    assert_eq!(top_holdings[0].ticker.as_deref(), Some("XFAKEPERF"));
    assert!((top_holdings[0].weight - 100.0).abs() < 1e-9);
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
    let output = cli_command(home.path())
        .args(["analyze", "composition", "--json"])
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

#[test]
fn composition_command_marks_unavailable_analysis_in_human_output() {
    let home = tempfile::tempdir().expect("temporary HOME should be created");
    run_cli_success(
        home.path(),
        &[
            "portfolio",
            "asset",
            "add",
            "--ticker",
            "IE00XFAKE001",
            "--name",
            "Unavailable Fund",
            "--type",
            "fund",
            "--asset-class",
            "equity",
            "--morningstar-code",
            "F000UNAVAILABLE",
        ],
    );
    let today = chrono::Local::now().format("%d-%m-%Y").to_string();
    run_cli_success(
        home.path(),
        &[
            "transaction",
            "buy",
            "--ticker",
            "IE00XFAKE001",
            "--date",
            &today,
            "--quantity",
            "1",
            "--price",
            "100",
        ],
    );

    let output = cli_command(home.path())
        .args(["analyze", "composition"])
        .output()
        .expect("rstock should run");

    assert!(
        output.status.success(),
        "rstock failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("Value-dependent composition: unavailable"));
    assert!(stdout.contains("Current position market data limitations"));
    assert!(stdout.contains("IE00XFAKE001"));
    assert!(!stdout.contains("NAV market data limitations"));
}

fn run_cli_success(home: &Path, args: &[&str]) {
    let output = cli_command(home)
        .args(args)
        .output()
        .expect("rstock should run");
    assert!(
        output.status.success(),
        "rstock failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn cli_command(home: &Path) -> Command {
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
