mod common;

use rstock::services::monitor::{compute_momentum, compute_relationship};

#[test]
fn test_rsi_all_gains() {
    let prices: Vec<f64> = (0..30).map(|i| 100.0 + i as f64).collect();
    let m = compute_momentum(&prices);
    let rsi = m.rsi_14.unwrap();
    assert!(
        rsi > 95.0,
        "RSI for all gains should be near 100, got {rsi}"
    );
}

#[test]
fn test_rsi_all_losses() {
    let prices: Vec<f64> = (0..30).map(|i| 200.0 - i as f64).collect();
    let m = compute_momentum(&prices);
    let rsi = m.rsi_14.unwrap();
    assert!(rsi < 5.0, "RSI for all losses should be near 0, got {rsi}");
}

#[test]
fn test_rsi_mixed_returns_in_range() {
    let prices: Vec<f64> = (0..30)
        .map(|i| 100.0 + (i as f64 * 0.5).sin() * 10.0)
        .collect();
    let m = compute_momentum(&prices);
    let rsi = m.rsi_14.unwrap();
    assert!(
        (0.0..=100.0).contains(&rsi),
        "RSI should be 0-100, got {rsi}"
    );
}

#[test]
fn test_rsi_insufficient_data() {
    let prices = vec![100.0; 10];
    let m = compute_momentum(&prices);
    assert!(m.rsi_14.is_none());
}

#[test]
fn test_sma_basic() {
    let prices = vec![10.0, 20.0, 30.0, 40.0, 50.0];
    let m = compute_momentum(&prices);
    // SMA50 needs 50 data points
    assert!(m.sma_50.is_none());
    // SMA200 needs 200 data points
    assert!(m.sma_200.is_none());
}

#[test]
fn test_sma_50_correct_value() {
    let mut prices = vec![100.0; 49];
    prices.push(200.0);
    let m = compute_momentum(&prices);
    let sma = m.sma_50.unwrap();
    // (49 * 100 + 200) / 50 = 5100 / 50 = 102
    assert!(
        (sma - 102.0).abs() < 1e-9,
        "SMA(50) should be 102.0, got {sma}"
    );
}

#[test]
fn test_sma_signals_above() {
    // Last price is above SMA50
    let mut prices: Vec<f64> = (0..50).map(|_| 100.0).collect();
    prices.push(150.0);
    let m = compute_momentum(&prices);
    assert_eq!(m.sma_50_signal.as_deref(), Some("Above"));
}

#[test]
fn test_sma_signals_below() {
    // Last price is below SMA50
    let mut prices: Vec<f64> = (0..50).map(|_| 100.0).collect();
    prices.push(50.0);
    let m = compute_momentum(&prices);
    assert_eq!(m.sma_50_signal.as_deref(), Some("Below"));
}

#[test]
fn test_macd_insufficient_data() {
    let prices = vec![100.0; 20];
    let m = compute_momentum(&prices);
    assert!(m.macd_line.is_none());
    assert!(m.macd_signal.is_none());
    assert!(m.macd_histogram.is_none());
}

#[test]
fn test_macd_computes_with_enough_data() {
    let prices: Vec<f64> = (0..60).map(|i| 100.0 + (i as f64).sin() * 10.0).collect();
    let m = compute_momentum(&prices);
    assert!(m.macd_line.is_some());
    assert!(m.macd_signal.is_some());
    assert!(m.macd_histogram.is_some());
    assert!(m.macd_signal_text.is_some());
}

#[test]
fn test_macd_histogram_equals_line_minus_signal() {
    let prices: Vec<f64> = (0..60).map(|i| 100.0 + (i as f64).sin() * 10.0).collect();
    let m = compute_momentum(&prices);
    let hist = m.macd_histogram.unwrap();
    let expected = m.macd_line.unwrap() - m.macd_signal.unwrap();
    assert!(
        (hist - expected).abs() < 1e-9,
        "Histogram should equal MACD - Signal"
    );
}

#[test]
fn test_macd_signal_text_is_bullish_or_bearish() {
    let prices: Vec<f64> = (0..60).map(|i| 100.0 + i as f64 * 2.0).collect();
    let m = compute_momentum(&prices);
    let text = m.macd_signal_text.as_deref().unwrap();
    assert!(
        text == "Bullish" || text == "Bearish",
        "MACD signal should be Bullish or Bearish, got {text}"
    );
}

#[test]
fn test_golden_cross_detection() {
    // 200 days at low, then 51 days rising sharply -> SMA50 > SMA200
    let mut prices = vec![100.0; 200];
    for i in 0..51 {
        prices.push(100.0 + i as f64 * 5.0);
    }
    let m = compute_momentum(&prices);
    assert!(m.sma_50.unwrap() > m.sma_200.unwrap());
    let cross = m.golden_death_cross.as_deref().unwrap();
    assert!(
        cross == "Golden Cross" || cross == "SMA50 > SMA200",
        "Expected golden cross signal, got {cross}"
    );
}

#[test]
fn test_death_cross_detection() {
    // 200 days at high, then 51 days dropping sharply -> SMA50 < SMA200
    let mut prices = vec![200.0; 200];
    for i in 0..51 {
        prices.push(200.0 - i as f64 * 5.0);
    }
    let m = compute_momentum(&prices);
    assert!(m.sma_50.unwrap() < m.sma_200.unwrap());
    let cross = m.golden_death_cross.as_deref().unwrap();
    assert!(
        cross == "Death Cross" || cross == "SMA50 < SMA200",
        "Expected death cross signal, got {cross}"
    );
}

#[test]
fn test_relationship_perfectly_correlated() {
    let stock: Vec<(String, f64)> = (0..30)
        .map(|i| (format!("2026-01-{:02}", i + 1), 100.0 + i as f64 * 2.0))
        .collect();
    let sector: Vec<(String, f64)> = (0..30)
        .map(|i| (format!("2026-01-{:02}", i + 1), 50.0 + i as f64))
        .collect();
    let rel = compute_relationship(&stock, &sector);
    assert!(rel.relative_strength_current.is_some());
    assert!(rel.beta_vs_sector.is_some());
    let corr = rel.correlation.unwrap();
    assert!(
        corr > 0.99,
        "Perfectly correlated linear series should have correlation ~1, got {corr}"
    );
}

#[test]
fn test_relationship_relative_strength_increases_when_stock_outperforms() {
    // Stock doubles, sector stays flat
    let stock: Vec<(String, f64)> = (0..30)
        .map(|i| (format!("2026-01-{:02}", i + 1), 100.0 + i as f64 * 3.0))
        .collect();
    let sector: Vec<(String, f64)> = (0..30)
        .map(|i| (format!("2026-01-{:02}", i + 1), 100.0))
        .collect();
    let rel = compute_relationship(&stock, &sector);
    let change = rel.relative_strength_change.unwrap();
    assert!(
        change > 0.0,
        "Relative strength should increase when stock outperforms, got {change}"
    );
}

#[test]
fn test_relationship_insufficient_data() {
    let stock = vec![("2026-01-01".to_owned(), 100.0)];
    let sector = vec![("2026-01-01".to_owned(), 50.0)];
    let rel = compute_relationship(&stock, &sector);
    assert!(rel.beta_vs_sector.is_none());
    assert!(rel.correlation.is_none());
}

#[test]
fn test_relationship_no_overlapping_dates() {
    let stock = vec![("2026-01-01".to_owned(), 100.0)];
    let sector = vec![("2026-02-01".to_owned(), 50.0)];
    let rel = compute_relationship(&stock, &sector);
    assert!(rel.relative_strength_current.is_none());
}

#[test]
fn test_relationship_beta_near_one_for_identical_series() {
    let base_date = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let stock: Vec<(String, f64)> = (0..60)
        .map(|i| {
            let date = base_date + chrono::Duration::days(i);
            (
                date.format("%Y-%m-%d").to_string(),
                100.0 + (i as f64).sin() * 10.0,
            )
        })
        .collect();
    let sector = stock.clone();
    let rel = compute_relationship(&stock, &sector);
    let beta = rel.beta_vs_sector.unwrap();
    assert!(
        (beta - 1.0).abs() < 0.01,
        "Beta should be ~1 for identical series, got {beta}"
    );
}

#[tokio::test]
async fn test_watchlist_crud() {
    let db = common::setup_test_db().await;

    use rstock::db::repos::watchlist_repo;

    // Initially empty
    let items = watchlist_repo::find_all(&db).await.unwrap();
    assert!(items.is_empty());

    // Insert
    watchlist_repo::insert(&db, "MSFT", "XLK").await.unwrap();
    watchlist_repo::insert(&db, "AAPL", "XLK").await.unwrap();

    // Find all
    let items = watchlist_repo::find_all(&db).await.unwrap();
    assert_eq!(items.len(), 2);

    // Find by ticker
    let item = watchlist_repo::find_by_ticker(&db, "MSFT")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.ticker, "MSFT");
    assert_eq!(item.sector_etf_ticker, "XLK");

    // Not found
    let missing = watchlist_repo::find_by_ticker(&db, "GOOG").await.unwrap();
    assert!(missing.is_none());

    // Delete
    let deleted = watchlist_repo::delete_by_ticker(&db, "MSFT").await.unwrap();
    assert!(deleted);

    let items = watchlist_repo::find_all(&db).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].ticker, "AAPL");

    // Delete non-existent
    let not_deleted = watchlist_repo::delete_by_ticker(&db, "MSFT").await.unwrap();
    assert!(!not_deleted);
}
