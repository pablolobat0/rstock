pub mod common;

use chrono::NaiveDate;
use common::{
    get_asset_snapshots, get_portfolio_snapshot, insert_asset, insert_daily_price,
    insert_dividend_transaction, insert_transaction, MockMarketDataSources,
};
use rstock::services::nav;

/// Cash dividend increases `total_value` (and therefore NAV) without changing
/// `outstanding_shares`.
#[tokio::test]
async fn test_cash_dividend_increases_nav() {
    let db = common::setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "FakeStock", "stock", "EUR").await;

    // Buy 10 shares @ 100 on day 1
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;

    // Price stays at 100
    insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;

    // Dividend of 50 total on day 2
    insert_dividend_transaction(&db, asset_id, "2025-01-03", 50.0, 0.0).await;

    let market_data = common::market_data_at(
        &MockMarketDataSources::new(),
        NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(),
    );
    nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    // Day 1: asset_value=1000, total_value=1000, NAV=100
    let snap1 = get_portfolio_snapshot(&db, "2025-01-02").await.unwrap();
    assert!((snap1.asset_value - 1000.0).abs() < 0.01);
    assert!((snap1.total_value - 1000.0).abs() < 0.01);
    assert!((snap1.nav - 100.0).abs() < 0.01);

    // Day 2: asset_value=1000, total_value=1050 (1000 + 50 dividend), NAV=105
    let snap2 = get_portfolio_snapshot(&db, "2025-01-03").await.unwrap();
    assert!((snap2.asset_value - 1000.0).abs() < 0.01);
    assert!((snap2.total_value - 1050.0).abs() < 0.01);
    assert!((snap2.outstanding_shares - snap1.outstanding_shares).abs() < 0.01); // unchanged
    assert!((snap2.nav - 105.0).abs() < 0.01);
}

/// Dividend with fees: only net amount (amount - fees) is added to cash.
#[tokio::test]
async fn test_dividend_with_fees() {
    let db = common::setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "FakeStock", "stock", "EUR").await;

    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;

    // Dividend 50 with 10 in fees → net 40
    insert_dividend_transaction(&db, asset_id, "2025-01-03", 50.0, 10.0).await;

    let market_data = common::market_data_at(
        &MockMarketDataSources::new(),
        NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(),
    );
    nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    let snap = get_portfolio_snapshot(&db, "2025-01-03").await.unwrap();
    // total_value = 1000 (assets) + 40 (net dividend)
    assert!((snap.total_value - 1040.0).abs() < 0.01);
    assert!((snap.nav - 104.0).abs() < 0.01);
}

/// Dividend does not change asset holdings.
#[tokio::test]
async fn test_dividend_does_not_change_holdings() {
    let db = common::setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "FakeStock", "stock", "EUR").await;

    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;

    insert_dividend_transaction(&db, asset_id, "2025-01-03", 50.0, 0.0).await;

    let market_data = common::market_data_at(
        &MockMarketDataSources::new(),
        NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(),
    );
    nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    let asset_snaps = get_asset_snapshots(&db, "2025-01-03").await;
    assert_eq!(asset_snaps.len(), 1);
    assert!((asset_snaps[0].quantity - 10.0).abs() < 0.01);
}

/// Accumulated cash is preserved across incremental rebuilds.
#[tokio::test]
async fn test_incremental_rebuild_preserves_cash_balance() {
    let db = common::setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "FakeStock", "stock", "EUR").await;

    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-06", 100.0, false).await;

    insert_dividend_transaction(&db, asset_id, "2025-01-03", 50.0, 0.0).await;

    let initial_market_data = common::market_data_at(
        &MockMarketDataSources::new(),
        NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(),
    );

    // Build up to day 2
    nav::ensure_portfolio_history(&db, &initial_market_data)
        .await
        .unwrap();

    let snap_day2 = get_portfolio_snapshot(&db, "2025-01-03").await.unwrap();
    assert!((snap_day2.total_value - 1050.0).abs() < 0.01);

    // The later fixed clock makes the public readiness interface extend history incrementally.
    let later_market_data = common::market_data_at(
        &MockMarketDataSources::new(),
        NaiveDate::from_ymd_opt(2025, 1, 7).unwrap(),
    );
    nav::ensure_portfolio_history(&db, &later_market_data)
        .await
        .unwrap();

    // Day 3: cash balance should carry forward → total_value = 1000 + 50 = 1050
    let snap_day3 = get_portfolio_snapshot(&db, "2025-01-06").await.unwrap();
    assert!((snap_day3.total_value - 1050.0).abs() < 0.01);
    assert!((snap_day3.nav - 105.0).abs() < 0.01);
}
