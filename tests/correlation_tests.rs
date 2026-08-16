mod common;

use common::{
    insert_asset, insert_daily_price, insert_etf_asset, insert_exchange_rate, insert_fund_asset,
    insert_portfolio_snapshot, insert_transaction, setup_test_db, MockMarketDataSources,
};
use rstock::constants::BENCHMARK_TICKER;
use rstock::db::repos::asset_repo;
use rstock::models::{MarketDataSubject, StockInfo};
use rstock::services::analytics::{
    compute_all_period_metrics, compute_correlation_data, compute_rolling_correlation_data,
};
use rstock::services::metrics::{
    align_return_series_with_dates, align_return_series_with_dates_unfiltered,
    compute_rolling_correlation, summarize_rolling_correlation,
};
use serde_json::{json, Value};

fn correlation_market_data(
    sources: &MockMarketDataSources,
) -> rstock::services::market_data::MarketData {
    common::market_data_at(
        sources,
        chrono::NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
    )
}

fn correlation_envelope<T: serde::Serialize>(command: &str, result: &T) -> (String, Value) {
    let mut output = Vec::new();
    rstock::cli::output::write_json(&mut output, command, result)
        .expect("correlation JSON should serialize");
    let text = String::from_utf8(output).expect("correlation JSON should be UTF-8");
    assert_eq!(text.lines().count(), 1);
    let value = serde_json::from_str(&text).expect("correlation output should be valid JSON");
    (text, value)
}

fn seed_benchmark_market_data(fetcher: &mut MockMarketDataSources, days: usize) {
    let benchmark_prices: Vec<(String, f64)> = (0..days)
        .map(|i| {
            let date = (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
                + chrono::Duration::days(i as i64))
            .format("%Y-%m-%d")
            .to_string();
            (date, 200.0 + i as f64)
        })
        .collect();
    let benchmark_fx: Vec<(String, f64)> = (0..days)
        .map(|i| {
            let date = (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
                + chrono::Duration::days(i as i64))
            .format("%Y-%m-%d")
            .to_string();
            (date, 0.92)
        })
        .collect();

    fetcher
        .historical_prices
        .insert(BENCHMARK_TICKER.to_owned(), benchmark_prices);
    fetcher
        .exchange_rates
        .insert("USDEUR".to_owned(), benchmark_fx);
}

fn seed_source_prices(
    fetcher: &mut MockMarketDataSources,
    ticker: &str,
    base_price: f64,
    days: usize,
) {
    let prices: Vec<(String, f64)> = (0..days)
        .map(|i| {
            let date = (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
                + chrono::Duration::days(i as i64))
            .format("%Y-%m-%d")
            .to_string();
            (date, base_price + i as f64)
        })
        .collect();

    fetcher.historical_prices.insert(ticker.to_owned(), prices);
}

/// Two perfectly correlated EUR assets (prices move in lockstep)
/// should have correlation = 1.0
#[tokio::test]
async fn test_perfectly_correlated_assets() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_benchmark_market_data(&mut fetcher, 25);

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    let id_b = insert_asset(&db, "XFAKE2", "Fake B", "stock", "EUR").await;

    insert_transaction(&db, id_a, "2025-01-01", 1.0, 100.0, 0.0).await;
    insert_transaction(&db, id_b, "2025-01-01", 1.0, 50.0, 0.0).await;

    // Both assets move identically in percentage terms
    let base_a = 100.0;
    let base_b = 50.0;
    let multipliers = [
        1.00, 1.02, 1.01, 1.03, 0.99, 1.01, 1.04, 0.98, 1.02, 1.00, 1.03, 1.01, 0.97, 1.02, 1.05,
        0.99, 1.01, 1.03, 0.98, 1.02, 1.04, 1.00, 0.99, 1.03, 1.01,
    ];

    for (i, &m) in multipliers.iter().enumerate() {
        let date = format!("2025-01-{:02}", i + 1);
        insert_daily_price(&db, id_a, &date, base_a * m, false).await;
        insert_daily_price(&db, id_b, &date, base_b * m, false).await;
    }

    let matrix = compute_correlation_data(
        &db,
        "2025-01-01",
        "2025-01-25",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    // Find XFAKE1 and XFAKE2 indices
    let idx_a = matrix.names.iter().position(|l| l == "Fake A").unwrap();
    let idx_b = matrix.names.iter().position(|l| l == "Fake B").unwrap();

    let corr = matrix.matrix[idx_a][idx_b].unwrap();
    assert!(
        (corr - 1.0).abs() < 0.01,
        "expected ~1.0 for perfectly correlated assets, got {corr}"
    );
    assert_eq!(matrix.matrix[idx_b][idx_a], matrix.matrix[idx_a][idx_b]);
}

/// Two inversely correlated EUR assets should have correlation close to -1.0
#[tokio::test]
async fn test_negatively_correlated_assets() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_benchmark_market_data(&mut fetcher, 25);

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    let id_b = insert_asset(&db, "XFAKE2", "Fake B", "stock", "EUR").await;

    insert_transaction(&db, id_a, "2025-01-01", 1.0, 100.0, 0.0).await;
    insert_transaction(&db, id_b, "2025-01-01", 1.0, 100.0, 0.0).await;

    // A goes up when B goes down and vice versa
    let changes = [
        0.02, -0.01, 0.03, -0.02, 0.01, -0.03, 0.02, -0.01, 0.03, -0.02, 0.01, -0.03, 0.02, -0.01,
        0.03, -0.02, 0.01, -0.03, 0.02, -0.01, 0.03, -0.02, 0.01, -0.03,
    ];

    let mut price_a = 100.0;
    let mut price_b = 100.0;
    insert_daily_price(&db, id_a, "2025-01-01", price_a, false).await;
    insert_daily_price(&db, id_b, "2025-01-01", price_b, false).await;

    for (i, &change) in changes.iter().enumerate() {
        price_a *= 1.0 + change;
        price_b *= 1.0 - change; // opposite direction
        let date = format!("2025-01-{:02}", i + 2);
        insert_daily_price(&db, id_a, &date, price_a, false).await;
        insert_daily_price(&db, id_b, &date, price_b, false).await;
    }

    let matrix = compute_correlation_data(
        &db,
        "2025-01-01",
        "2025-01-25",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    let idx_a = matrix.names.iter().position(|l| l == "Fake A").unwrap();
    let idx_b = matrix.names.iter().position(|l| l == "Fake B").unwrap();

    let corr = matrix.matrix[idx_a][idx_b].unwrap();
    assert!(
        corr < -0.9,
        "expected strongly negative correlation, got {corr}"
    );
}

/// Diagonal should always be 1.0
#[tokio::test]
async fn test_diagonal_is_one() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_benchmark_market_data(&mut fetcher, 25);

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    insert_transaction(&db, id_a, "2025-01-01", 1.0, 100.0, 0.0).await;

    for i in 1..=25 {
        let date = format!("2025-01-{i:02}");
        let price = 100.0 + (i as f64) * 0.5;
        insert_daily_price(&db, id_a, &date, price, false).await;
    }

    let matrix = compute_correlation_data(
        &db,
        "2025-01-01",
        "2025-01-25",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    for i in 0..matrix.names.len() {
        let diag = matrix.matrix[i][i];
        assert_eq!(diag, Some(1.0), "diagonal [{i}][{i}] should be 1.0");
    }
}

/// USD assets should have their prices converted to EUR before correlation
#[tokio::test]
async fn test_usd_asset_uses_eur_conversion() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_benchmark_market_data(&mut fetcher, 25);

    // One EUR asset, one USD asset with same percentage moves
    let id_eur = insert_asset(&db, "XFAKE1", "Fake EUR", "stock", "EUR").await;
    let id_usd = insert_asset(&db, "XFAKE2", "Fake USD", "stock", "USD").await;

    insert_transaction(&db, id_eur, "2025-01-01", 1.0, 100.0, 0.0).await;
    insert_transaction(&db, id_usd, "2025-01-01", 1.0, 100.0, 0.0).await;

    // Constant exchange rate = price moves are identical in EUR
    for i in 1..=25 {
        let date = format!("2025-01-{i:02}");
        let price = 100.0 + (i as f64);
        insert_daily_price(&db, id_eur, &date, price, false).await;
        insert_daily_price(&db, id_usd, &date, price, false).await;
        insert_exchange_rate(&db, "USD", "EUR", &date, 0.92).await;
    }

    let matrix = compute_correlation_data(
        &db,
        "2025-01-01",
        "2025-01-25",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    let idx_eur = matrix.names.iter().position(|l| l == "Fake EUR").unwrap();
    let idx_usd = matrix.names.iter().position(|l| l == "Fake USD").unwrap();

    // With constant FX, EUR-converted returns are identical → correlation ~1.0
    let corr = matrix.matrix[idx_eur][idx_usd].unwrap();
    assert!(
        (corr - 1.0).abs() < 0.01,
        "expected ~1.0 with constant FX, got {corr}"
    );
}

#[tokio::test]
async fn test_correlation_market_data_returns_tracked_and_benchmark_series_separately() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_source_prices(&mut fetcher, "XFAKE1", 100.0, 25);
    seed_source_prices(&mut fetcher, "XFAKE2", 50.0, 25);
    seed_benchmark_market_data(&mut fetcher, 25);

    let id_eur = insert_asset(&db, "XFAKE1", "Fake EUR", "stock", "EUR").await;
    let id_usd = insert_asset(&db, "XFAKE2", "Fake USD", "stock", "USD").await;

    let market_data = correlation_market_data(&fetcher);
    let tracked_assets = asset_repo::find_by_ids(&db, [id_eur, id_usd].into_iter())
        .await
        .unwrap();

    let result = market_data
        .correlation_market_data(&db, tracked_assets, "2025-01-01", "2025-01-25")
        .await
        .unwrap();

    assert_eq!(result.requested_start_date, "2025-01-01");
    assert_eq!(result.requested_end_date, "2025-01-25");
    assert_eq!(result.tracked_asset_series.len(), 2);
    assert_eq!(result.benchmark_series.name, "MSCI ACWI Benchmark");
    assert!(!result
        .tracked_asset_series
        .iter()
        .any(|series| series.name == result.benchmark_series.name));

    let usd_series = result
        .tracked_asset_series
        .iter()
        .find(|series| series.name == "Fake USD")
        .unwrap();
    assert_eq!(usd_series.prices.len(), 25);
    assert!((usd_series.prices[0].1 - 46.0).abs() < 1e-9);
}

/// Assets with insufficient data should appear in warnings and have None correlation
#[tokio::test]
async fn test_insufficient_data_produces_warning() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_benchmark_market_data(&mut fetcher, 5);

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    insert_transaction(&db, id_a, "2025-01-01", 1.0, 100.0, 0.0).await;

    // Only 5 days of data — below MIN_DATA_POINTS (20)
    for i in 1..=5 {
        let date = format!("2025-01-{i:02}");
        insert_daily_price(&db, id_a, &date, 100.0 + i as f64, false).await;
    }

    let matrix = compute_correlation_data(
        &db,
        "2025-01-01",
        "2025-01-05",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    assert!(
        matrix.warnings.contains(&"Fake A".to_string()),
        "Fake A should be in warnings due to insufficient data"
    );
}

#[tokio::test]
async fn test_correlation_matrix_carries_market_data_limitations() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_benchmark_market_data(&mut fetcher, 25);

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    seed_source_prices(&mut fetcher, "XFAKE1", 100.0, 25);
    insert_transaction(&db, id_a, "2025-01-01", 1.0, 100.0, 0.0).await;

    let matrix = compute_correlation_data(
        &db,
        "2025-01-01",
        "2025-02-10",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    assert!(matrix.market_data_limitations.iter().any(|limitation| {
        matches!(
            &limitation.subject,
            MarketDataSubject::Asset { ticker, .. } if ticker == "XFAKE1"
        )
    }));
}

#[tokio::test]
async fn test_correlation_matrix_json_preserves_nulls_warnings_and_limitations() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_benchmark_market_data(&mut fetcher, 5);
    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    seed_source_prices(&mut fetcher, "XFAKE1", 100.0, 5);
    insert_transaction(&db, id_a, "2025-01-01", 1.0, 100.0, 0.0).await;

    let matrix = compute_correlation_data(
        &db,
        "2025-01-01",
        "2025-02-10",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();
    let (text, envelope) = correlation_envelope("analyze.correlation.matrix", &matrix);

    assert_eq!(envelope["command"], "analyze.correlation.matrix");
    assert!(envelope["data"]["names"]
        .as_array()
        .unwrap()
        .contains(&json!("Fake A")));
    assert!(envelope["data"]["matrix"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|row| row.as_array().unwrap().iter())
        .any(Value::is_null));
    assert!(envelope["data"]["warnings"]
        .as_array()
        .unwrap()
        .contains(&json!("Fake A")));
    let limitations = envelope["data"]["market_data_limitations"]
        .as_array()
        .unwrap();
    assert!(!limitations.is_empty());
    assert!(limitations.iter().any(|limitation| {
        limitation["subject"]["type"] == "asset"
            && limitation["subject"]["ticker"] == "XFAKE1"
            && limitation["latest_available_date"] == "2025-01-05"
            && limitation["requested_end_date"] == "2025-02-10"
    }));
    assert!(!text.contains("\u{1b}["));
}

#[test]
fn test_align_return_series_with_dates_sorts_chronologically() {
    let a = std::collections::HashMap::from([
        ("2025-01-03".to_string(), 0.03),
        ("2025-01-01".to_string(), 0.01),
        ("2025-01-02".to_string(), 0.02),
    ]);
    let b = std::collections::HashMap::from([
        ("2025-01-02".to_string(), 0.12),
        ("2025-01-01".to_string(), 0.11),
        ("2025-01-03".to_string(), 0.13),
    ]);

    let aligned = align_return_series_with_dates(&a, &b);
    let dates: Vec<_> = aligned.iter().map(|(date, _, _)| date.as_str()).collect();
    assert_eq!(dates, vec!["2025-01-01", "2025-01-02", "2025-01-03"]);
}

#[test]
fn test_compute_rolling_correlation_perfect_positive() {
    let aligned: Vec<(String, f64, f64)> = (0..65)
        .map(|i| {
            (
                format!("2025-01-{:02}", i + 1),
                0.01 + (i as f64) * 0.0001,
                0.01 + (i as f64) * 0.0001,
            )
        })
        .collect();

    let points = compute_rolling_correlation(&aligned);
    assert_eq!(points.len(), 6);
    assert!(points.iter().all(|(_, corr)| (*corr - 1.0).abs() < 1e-9));
}

#[test]
fn test_unfiltered_rolling_alignment_keeps_zero_return_days() {
    let a = std::collections::HashMap::from([
        ("2025-01-01".to_string(), 0.0),
        ("2025-01-02".to_string(), 0.01),
    ]);
    let b = std::collections::HashMap::from([
        ("2025-01-01".to_string(), 0.0),
        ("2025-01-02".to_string(), 0.02),
    ]);

    let aligned = align_return_series_with_dates_unfiltered(&a, &b);
    let dates: Vec<_> = aligned.iter().map(|(date, _, _)| date.as_str()).collect();
    assert_eq!(dates, vec!["2025-01-01", "2025-01-02"]);
}

#[test]
fn test_summarize_rolling_correlation_values() {
    let points = vec![
        ("2025-01-01".to_string(), 0.2),
        ("2025-01-02".to_string(), -0.1),
        ("2025-01-03".to_string(), 0.5),
    ];

    let (latest, min, max, average) = summarize_rolling_correlation(&points);
    assert_eq!(latest, Some(0.5));
    assert_eq!(min, Some(-0.1));
    assert_eq!(max, Some(0.5));
    assert!((average.unwrap() - 0.2).abs() < 1e-9);
}

#[tokio::test]
async fn test_rolling_correlation_for_pair() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    insert_asset(&db, "XFAKE2", "Fake B", "stock", "EUR").await;

    let prices_a: Vec<(String, f64)> = (0..120)
        .map(|i| {
            let date = format!(
                "{}",
                (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap() + chrono::Duration::days(i))
                    .format("%Y-%m-%d")
            );
            let price_a = 100.0 + i as f64 * 0.5 + (i as f64 / 7.0).sin();
            (date, price_a)
        })
        .collect();
    let prices_b: Vec<(String, f64)> = (0..120)
        .map(|i| {
            let date = format!(
                "{}",
                (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap() + chrono::Duration::days(i))
                    .format("%Y-%m-%d")
            );
            let price_b = 50.0 + i as f64 * 0.25 + (i as f64 / 7.0).sin() * 0.5;
            (date, price_b)
        })
        .collect();

    fetcher
        .historical_prices
        .insert("XFAKE1".to_string(), prices_a);
    fetcher
        .historical_prices
        .insert("XFAKE2".to_string(), prices_b);
    fetcher.stock_info.insert(
        "XFAKE1".to_string(),
        StockInfo {
            ticker: "XFAKE1".to_string(),
            name: Some("Fake A".to_string()),
            currency: Some("EUR".to_string()),
            current_price: None,
            previous_close: None,
            day_range: None,
            fifty_two_week_range: None,
            volume: None,
            avg_volume: None,
            market_cap: None,
            pe_ttm: None,
            eps_ttm: None,
            dividend_yield: None,
            sector: None,
            industry: None,
            country: None,
        },
    );
    fetcher.stock_info.insert(
        "XFAKE2".to_string(),
        StockInfo {
            ticker: "XFAKE2".to_string(),
            name: Some("Fake B".to_string()),
            currency: Some("EUR".to_string()),
            current_price: None,
            previous_close: None,
            day_range: None,
            fifty_two_week_range: None,
            volume: None,
            avg_volume: None,
            market_cap: None,
            pe_ttm: None,
            eps_ttm: None,
            dividend_yield: None,
            sector: None,
            industry: None,
            country: None,
        },
    );

    let result = compute_rolling_correlation_data(
        &db,
        "2025-01-01",
        "2025-04-30",
        "XFAKE1",
        "XFAKE2",
        "1Y",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    assert_eq!(result.left_name, "Fake A");
    assert_eq!(result.right_name, "Fake B");
    assert_eq!(result.requested_start_date, "2025-01-01");
    assert_eq!(result.requested_end_date, "2025-04-30");
    assert!(!result.points.is_empty());
    assert!(result.latest.is_some());
}

#[tokio::test]
async fn test_rolling_correlation_carries_market_data_limitations() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    insert_asset(&db, "XFAKE2", "Fake B", "stock", "EUR").await;
    seed_source_prices(&mut fetcher, "XFAKE1", 100.0, 120);
    seed_source_prices(&mut fetcher, "XFAKE2", 50.0, 120);

    let result = compute_rolling_correlation_data(
        &db,
        "2025-01-01",
        "2025-05-30",
        "XFAKE1",
        "XFAKE2",
        "1Y",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    assert!(!result.points.is_empty());
    assert!(result.market_data_limitations.iter().any(|limitation| {
        matches!(
            &limitation.subject,
            MarketDataSubject::Asset { ticker, .. } if ticker == "XFAKE1"
        )
    }));
}

#[tokio::test]
async fn test_rolling_correlation_json_preserves_context_summary_points_and_limitations() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    insert_asset(&db, "XFAKE2", "Fake B", "stock", "EUR").await;
    seed_source_prices(&mut fetcher, "XFAKE1", 100.0, 120);
    seed_source_prices(&mut fetcher, "XFAKE2", 50.0, 120);

    let result = compute_rolling_correlation_data(
        &db,
        "2025-01-01",
        "2025-05-30",
        "XFAKE1",
        "XFAKE2",
        "1Y",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();
    let (text, envelope) = correlation_envelope("analyze.correlation.rolling", &result);
    let data = &envelope["data"];

    assert_eq!(envelope["command"], "analyze.correlation.rolling");
    assert_eq!(data["left_name"], "Fake A");
    assert_eq!(data["right_name"], "Fake B");
    assert_eq!(data["period_label"], "1Y");
    assert_eq!(data["window_label"], "60D rolling");
    assert_eq!(data["requested_start_date"], "2025-01-01");
    assert_eq!(data["requested_end_date"], "2025-05-30");
    for metric in ["latest", "min", "max", "average"] {
        assert!(data[metric].is_number());
    }
    let points = data["points"].as_array().unwrap();
    assert!(!points.is_empty());
    assert!(points.iter().all(|point| {
        point
            .as_array()
            .is_some_and(|pair| pair.len() == 2 && pair[0].is_string() && pair[1].is_number())
    }));
    assert!(!data["market_data_limitations"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!text.contains("\u{1b}["));
}

#[tokio::test]
async fn test_insufficient_rolling_correlation_json_uses_normal_schema() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;
    insert_asset(&db, "XFAKE2", "Fake B", "stock", "EUR").await;
    seed_source_prices(&mut fetcher, "XFAKE1", 100.0, 20);
    seed_source_prices(&mut fetcher, "XFAKE2", 50.0, 20);

    let result = compute_rolling_correlation_data(
        &db,
        "2025-01-01",
        "2025-01-20",
        "XFAKE1",
        "XFAKE2",
        "30D",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();
    let (text, envelope) = correlation_envelope("analyze.correlation.rolling", &result);
    let data = &envelope["data"];

    assert_eq!(data["points"], json!([]));
    for metric in ["latest", "min", "max", "average"] {
        assert!(data[metric].is_null());
    }
    assert_eq!(data["left_name"], "Fake A");
    assert_eq!(data["right_name"], "Fake B");
    assert!(!text.contains("Not enough aligned data"));
    assert!(!text.contains("\u{1b}["));
}

#[tokio::test]
async fn test_period_metrics_carry_benchmark_market_data_limitations() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    seed_benchmark_market_data(&mut fetcher, 25);

    for i in 0..25 {
        let date = (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
            + chrono::Duration::days(i))
        .format("%Y-%m-%d")
        .to_string();
        insert_portfolio_snapshot(&db, &date, 100.0 + i as f64, 1.0).await;
    }

    let result = compute_all_period_metrics(
        &db,
        "2025-02-10",
        "2025-01-01",
        "2025-01-01",
        "2025-01-01",
        "2025-01-01",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    assert!(result.market_data_limitations.iter().any(|limitation| {
        matches!(
            &limitation.subject,
            MarketDataSubject::Asset { ticker, .. } if ticker == BENCHMARK_TICKER
        )
    }));
}

#[tokio::test]
async fn test_rolling_correlation_rejects_unknown_identifier() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    insert_asset(&db, "XFAKE1", "Fake A", "stock", "EUR").await;

    let prices: Vec<(String, f64)> = (0..120)
        .map(|i| {
            let date = (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
                + chrono::Duration::days(i))
            .format("%Y-%m-%d")
            .to_string();
            (date, 100.0 + i as f64 * 0.8)
        })
        .collect();
    fetcher
        .historical_prices
        .insert("XFAKE1".to_string(), prices);

    let result = compute_rolling_correlation_data(
        &db,
        "2025-01-01",
        "2025-04-30",
        "XFAKE1",
        "XUNKNOWN",
        "1Y",
        &correlation_market_data(&fetcher),
    )
    .await;
    let Err(err) = result else {
        panic!("unknown tracked asset should be rejected");
    };

    assert!(err
        .to_string()
        .contains("tracked asset 'XUNKNOWN' not found"));
}

#[tokio::test]
async fn test_rolling_correlation_uses_morningstar_code_for_funds_and_etfs() {
    let db = setup_test_db().await;
    let mut fetcher = MockMarketDataSources::new();
    insert_fund_asset(&db, "IE00XFAKE1", "Fake Fund", "EUR", "F00000FUND").await;
    insert_etf_asset(&db, "IE00XFAKE2", "Fake ETF", "EUR", "F00000ETF").await;

    let fund_prices: Vec<(String, f64)> = (0..120)
        .map(|i| {
            let date = (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
                + chrono::Duration::days(i))
            .format("%Y-%m-%d")
            .to_string();
            (date, 100.0 + i as f64 * 0.5)
        })
        .collect();
    let etf_prices: Vec<(String, f64)> = (0..120)
        .map(|i| {
            let date = (chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
                + chrono::Duration::days(i))
            .format("%Y-%m-%d")
            .to_string();
            (date, 50.0 + i as f64 * 0.25)
        })
        .collect();
    fetcher
        .historical_prices
        .insert("F00000FUND".to_string(), fund_prices);
    fetcher
        .historical_prices
        .insert("F00000ETF".to_string(), etf_prices);

    let result = compute_rolling_correlation_data(
        &db,
        "2025-01-01",
        "2025-04-30",
        "IE00XFAKE1",
        "IE00XFAKE2",
        "1Y",
        &correlation_market_data(&fetcher),
    )
    .await
    .unwrap();

    assert_eq!(result.left_name, "Fake Fund");
    assert_eq!(result.right_name, "Fake ETF");
    assert!(!result.points.is_empty());
}
