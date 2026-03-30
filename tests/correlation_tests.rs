mod common;

use common::{
    insert_asset, insert_daily_price, insert_exchange_rate, insert_transaction, setup_test_db,
    MockPriceFetcher,
};
use rstock::services::metrics::compute_correlation_matrix;
use rstock::services::nav::rebuild_portfolio_history;

/// Two perfectly correlated EUR assets (prices move in lockstep)
/// should have correlation = 1.0
#[tokio::test]
async fn test_perfectly_correlated_assets() {
    let db = setup_test_db().await;
    let fetcher = MockPriceFetcher::new();

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", None, "EUR").await;
    let id_b = insert_asset(&db, "XFAKE2", "Fake B", "stock", None, "EUR").await;

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

    // Build portfolio history
    let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 1, 25).unwrap();
    rebuild_portfolio_history(&db, start, end, None, &fetcher)
        .await
        .unwrap();

    let matrix = compute_correlation_matrix(&db, "2025-01-01", "2025-01-25", &fetcher)
        .await
        .unwrap();

    // Find XFAKE1 and XFAKE2 indices
    let idx_a = matrix.labels.iter().position(|l| l == "XFAKE1").unwrap();
    let idx_b = matrix.labels.iter().position(|l| l == "XFAKE2").unwrap();

    let corr = matrix.matrix[idx_a][idx_b].unwrap();
    assert!(
        (corr - 1.0).abs() < 0.01,
        "expected ~1.0 for perfectly correlated assets, got {corr}"
    );
}

/// Two inversely correlated EUR assets should have correlation close to -1.0
#[tokio::test]
async fn test_negatively_correlated_assets() {
    let db = setup_test_db().await;
    let fetcher = MockPriceFetcher::new();

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", None, "EUR").await;
    let id_b = insert_asset(&db, "XFAKE2", "Fake B", "stock", None, "EUR").await;

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

    let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 1, 25).unwrap();
    rebuild_portfolio_history(&db, start, end, None, &fetcher)
        .await
        .unwrap();

    let matrix = compute_correlation_matrix(&db, "2025-01-01", "2025-01-25", &fetcher)
        .await
        .unwrap();

    let idx_a = matrix.labels.iter().position(|l| l == "XFAKE1").unwrap();
    let idx_b = matrix.labels.iter().position(|l| l == "XFAKE2").unwrap();

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
    let fetcher = MockPriceFetcher::new();

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", None, "EUR").await;
    insert_transaction(&db, id_a, "2025-01-01", 1.0, 100.0, 0.0).await;

    for i in 1..=25 {
        let date = format!("2025-01-{i:02}");
        let price = 100.0 + (i as f64) * 0.5;
        insert_daily_price(&db, id_a, &date, price, false).await;
    }

    let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 1, 25).unwrap();
    rebuild_portfolio_history(&db, start, end, None, &fetcher)
        .await
        .unwrap();

    let matrix = compute_correlation_matrix(&db, "2025-01-01", "2025-01-25", &fetcher)
        .await
        .unwrap();

    for i in 0..matrix.labels.len() {
        let diag = matrix.matrix[i][i];
        assert_eq!(diag, Some(1.0), "diagonal [{i}][{i}] should be 1.0");
    }
}

/// USD assets should have their prices converted to EUR before correlation
#[tokio::test]
async fn test_usd_asset_uses_eur_conversion() {
    let db = setup_test_db().await;
    let fetcher = MockPriceFetcher::new();

    // One EUR asset, one USD asset with same percentage moves
    let id_eur = insert_asset(&db, "XFAKE1", "Fake EUR", "stock", None, "EUR").await;
    let id_usd = insert_asset(&db, "XFAKE2", "Fake USD", "stock", None, "USD").await;

    insert_transaction(&db, id_eur, "2025-01-01", 1.0, 100.0, 0.0).await;
    insert_transaction(&db, id_usd, "2025-01-01", 1.0, 100.0, 0.0).await;

    // Constant exchange rate = price moves are identical in EUR
    for i in 1..=25 {
        let date = format!("2025-01-{i:02}");
        let price = 100.0 + (i as f64);
        insert_daily_price(&db, id_eur, &date, price, false).await;
        insert_daily_price(&db, id_usd, &date, price, false).await;
        insert_exchange_rate(&db, "USDEUR", &date, 0.92).await;
    }

    let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 1, 25).unwrap();
    rebuild_portfolio_history(&db, start, end, None, &fetcher)
        .await
        .unwrap();

    let matrix = compute_correlation_matrix(&db, "2025-01-01", "2025-01-25", &fetcher)
        .await
        .unwrap();

    let idx_eur = matrix.labels.iter().position(|l| l == "XFAKE1").unwrap();
    let idx_usd = matrix.labels.iter().position(|l| l == "XFAKE2").unwrap();

    // With constant FX, EUR-converted returns are identical → correlation ~1.0
    let corr = matrix.matrix[idx_eur][idx_usd].unwrap();
    assert!(
        (corr - 1.0).abs() < 0.01,
        "expected ~1.0 with constant FX, got {corr}"
    );
}

/// Assets with insufficient data should appear in warnings and have None correlation
#[tokio::test]
async fn test_insufficient_data_produces_warning() {
    let db = setup_test_db().await;
    let fetcher = MockPriceFetcher::new();

    let id_a = insert_asset(&db, "XFAKE1", "Fake A", "stock", None, "EUR").await;
    insert_transaction(&db, id_a, "2025-01-01", 1.0, 100.0, 0.0).await;

    // Only 5 days of data — below MIN_DATA_POINTS (20)
    for i in 1..=5 {
        let date = format!("2025-01-{i:02}");
        insert_daily_price(&db, id_a, &date, 100.0 + i as f64, false).await;
    }

    let start = chrono::NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 1, 5).unwrap();
    rebuild_portfolio_history(&db, start, end, None, &fetcher)
        .await
        .unwrap();

    let matrix = compute_correlation_matrix(&db, "2025-01-01", "2025-01-05", &fetcher)
        .await
        .unwrap();

    assert!(
        matrix.warnings.contains(&"XFAKE1".to_string()),
        "XFAKE1 should be in warnings due to insufficient data"
    );
}
