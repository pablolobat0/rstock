mod common;

use chrono::NaiveDate;
use common::{
    get_asset_snapshots, get_portfolio_snapshot, insert_asset, insert_daily_price,
    insert_dividend_transaction, insert_transaction, MockPriceFetcher,
};
use rstock::services::nav;
use std::collections::HashMap;

use rstock::models::{Transaction, TxType};

/// Cash dividend increases total_value (and therefore NAV) without changing outstanding_shares.
#[tokio::test]
async fn test_cash_dividend_increases_nav() {
    let db = common::setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "FakeStock", "stock", None, "EUR").await;

    // Buy 10 shares @ 100 on day 1
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;

    // Price stays at 100
    insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;

    // Dividend of 50 total on day 2
    insert_dividend_transaction(&db, asset_id, "2025-01-03", 50.0, 0.0).await;

    let fetcher = MockPriceFetcher::new();
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    let end = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
    nav::rebuild_portfolio_history(&db, start, end, None, &fetcher)
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
    let asset_id = insert_asset(&db, "XFAKE1", "FakeStock", "stock", None, "EUR").await;

    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;

    // Dividend 50 with 10 in fees → net 40
    insert_dividend_transaction(&db, asset_id, "2025-01-03", 50.0, 10.0).await;

    let fetcher = MockPriceFetcher::new();
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    let end = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
    nav::rebuild_portfolio_history(&db, start, end, None, &fetcher)
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
    let asset_id = insert_asset(&db, "XFAKE1", "FakeStock", "stock", None, "EUR").await;

    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;

    insert_dividend_transaction(&db, asset_id, "2025-01-03", 50.0, 0.0).await;

    let fetcher = MockPriceFetcher::new();
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    let end = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
    nav::rebuild_portfolio_history(&db, start, end, None, &fetcher)
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
    let asset_id = insert_asset(&db, "XFAKE1", "FakeStock", "stock", None, "EUR").await;

    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;
    insert_daily_price(&db, asset_id, "2025-01-06", 100.0, false).await;

    insert_dividend_transaction(&db, asset_id, "2025-01-03", 50.0, 0.0).await;

    let fetcher = MockPriceFetcher::new();

    // Build up to day 2
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    let end = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
    nav::rebuild_portfolio_history(&db, start, end, None, &fetcher)
        .await
        .unwrap();

    let snap_day2 = get_portfolio_snapshot(&db, "2025-01-03").await.unwrap();
    assert!((snap_day2.total_value - 1050.0).abs() < 0.01);

    // Incremental rebuild from day 3 using prev snapshot
    let prev_snap = rstock::models::PortfolioSnapshot::from(snap_day2);
    let start2 = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
    let end2 = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
    nav::rebuild_portfolio_history(&db, start2, end2, Some(&prev_snap), &fetcher)
        .await
        .unwrap();

    // Day 3: cash balance should carry forward → total_value = 1000 + 50 = 1050
    let snap_day3 = get_portfolio_snapshot(&db, "2025-01-06").await.unwrap();
    assert!((snap_day3.total_value - 1050.0).abs() < 0.01);
    assert!((snap_day3.nav - 105.0).abs() < 0.01);
}

/// Dividend signed_quantity returns 0.0 (no holdings change).
#[test]
fn test_dividend_signed_quantity_is_zero() {
    let tx = Transaction {
        asset_id: 1,
        tx_type: TxType::Dividend,
        date: "2025-01-03".to_owned(),
        quantity: 1.0,
        price_cents: 5000,
        fees_cents: 0,
    };
    assert!((tx.signed_quantity()).abs() < f64::EPSILON);
}

/// Unit test: process_day_transactions with a dividend returns income.
#[tokio::test]
async fn test_process_day_transactions_dividend_pure() {
    let asset = rstock::models::Asset {
        id: 1,
        ticker: "XFAKE1".to_owned(),
        isin: None,
        name: "FakeStock".to_owned(),
        asset_type: rstock::models::AssetType::Stock,
        currency: "EUR".to_owned(),
    };
    let asset_map: HashMap<i32, &rstock::models::Asset> = HashMap::from([(1, &asset)]);
    let day_rates: HashMap<String, f64> = HashMap::new();

    // First buy to establish holdings and shares
    let buy_tx = Transaction {
        asset_id: 1,
        tx_type: TxType::Buy,
        date: "2025-01-02".to_owned(),
        quantity: 10.0,
        price_cents: 10000,
        fees_cents: 0,
    };
    let mut holdings: HashMap<i32, f64> = HashMap::new();
    let (os, nav_val, div_income) = nav::process_day_transactions(
        &vec![&buy_tx],
        &mut holdings,
        0.0,
        100.0,
        &asset_map,
        &day_rates,
    );
    assert!((os - 10.0).abs() < 0.01); // 1000/100 = 10
    assert!((div_income).abs() < 0.01); // no dividend

    // Now process a dividend
    let div_tx = Transaction {
        asset_id: 1,
        tx_type: TxType::Dividend,
        date: "2025-01-03".to_owned(),
        quantity: 1.0,
        price_cents: 5000, // 50.00 total
        fees_cents: 0,
    };
    let (os2, nav_val2, div_income2) = nav::process_day_transactions(
        &vec![&div_tx],
        &mut holdings,
        os,
        nav_val,
        &asset_map,
        &day_rates,
    );

    // Outstanding shares unchanged
    assert!((os2 - os).abs() < 0.01);
    // NAV unchanged (price hasn't been recalculated yet, that happens after)
    assert!((nav_val2 - nav_val).abs() < 0.01);
    // Dividend income returned
    assert!((div_income2 - 50.0).abs() < 0.01);
    // Holdings unchanged
    assert_eq!(*holdings.get(&1).unwrap(), 10.0);
}
