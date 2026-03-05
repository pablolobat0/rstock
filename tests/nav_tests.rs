mod common;

use std::collections::HashMap;

use chrono::NaiveDate;
use rstock::db::repos::portfolio_history_repo;
use rstock::models::Transaction;
use rstock::services::nav;

/// No transactions -> rebuild returns Ok, no portfolio_history rows.
#[tokio::test]
async fn test_empty_portfolio() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snapshots = common::get_all_snapshots(&db).await;
    assert!(snapshots.is_empty());
}

/// One buy -> NAV starts at 100.0, outstanding_shares = deposit / 100.
#[tokio::test]
async fn test_single_buy_initial_nav() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    // Buy 10 shares at $50 = $500 deposit, 0 fees
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    // EOD price is $50
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .expect("snapshot should exist");

    // deposit = 10 * 50 = 500
    // initial NAV = 100, shares_issued = 500 / 100 = 5
    assert_eq!(snap.outstanding_shares, 5.0);
    // EOD: asset_value = 10 * 50 = 500, nav = 500 / 5 = 100
    assert!((snap.nav - 100.0).abs() < 0.01);
    assert!((snap.asset_value - 500.0).abs() < 0.01);
    assert!((snap.total_value - 500.0).abs() < 0.01);
}

/// Buy day 1, price doubles day 2 -> NAV doubles, shares unchanged.
#[tokio::test]
async fn test_nav_reflects_price_change() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap_d1 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    let snap_d2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();

    // Shares unchanged
    assert_eq!(snap_d1.outstanding_shares, snap_d2.outstanding_shares);
    // NAV doubled: 100 -> 200
    assert!((snap_d2.nav - 200.0).abs() < 0.01);
    // Asset value doubled: 500 -> 1000
    assert!((snap_d2.asset_value - 1000.0).abs() < 0.01);
}

/// Buy day 1, buy day 5 -> NAV on day 5 uses previous day's EOD NAV
/// for share issuance; outstanding_shares increases.
#[tokio::test]
async fn test_second_buy_no_nav_jump() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    // Day 1: buy 10 @ $50
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    // Day 5: buy 10 @ $60
    common::insert_transaction(&db, asset_id, "2025-01-06", 10.0, 60.0, 0.0).await;

    // Prices: stable at 50, then 60 on day 5
    for (date, price) in [
        ("2025-01-02", 50.0),
        ("2025-01-03", 50.0),
        ("2025-01-04", 50.0),
        ("2025-01-05", 50.0),
        ("2025-01-06", 60.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap_d4 = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .unwrap();
    let snap_d5 = common::get_portfolio_snapshot(&db, "2025-01-06")
        .await
        .unwrap();

    // Day 4 (before second buy): 10 shares @ 50 = 500, 5 outstanding, NAV = 100
    assert_eq!(snap_d4.outstanding_shares, 5.0);
    assert!((snap_d4.nav - 100.0).abs() < 0.01);

    // Day 5: price jumped to 60
    // The NAV engine uses previous day's EOD NAV for share issuance.
    // Pre-buy NAV (from Jan 5 EOD) = 100 (price was still 50)
    // Second buy deposit = 10 * 60 = 600, shares_issued = 600 / 100 = 6
    // outstanding = 5 + 6 = 11, holdings = 20
    // EOD: 20 * 60 = 1200, NAV = 1200 / 11 ~ 109.09
    assert!(snap_d5.outstanding_shares > snap_d4.outstanding_shares);
    assert!((snap_d5.outstanding_shares - 11.0).abs() < 0.01);
    assert!((snap_d5.asset_value - 1200.0).abs() < 0.01);
    assert!((snap_d5.nav - (1200.0 / 11.0)).abs() < 0.01);
}

/// Two buys on same day -> shares accumulate correctly.
#[tokio::test]
async fn test_same_day_multiple_buys() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    // Two buys on same day
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 5.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();

    // First buy: deposit=500, NAV=100, shares=5, holdings=10
    // Second buy: deposit=250, NAV=100, shares_issued=2.5, outstanding=7.5, holdings=15
    // EOD: 15 * 50 = 750, NAV = 750 / 7.5 = 100
    assert!((snap.outstanding_shares - 7.5).abs() < 0.01);
    assert!((snap.asset_value - 750.0).abs() < 0.01);
    assert!((snap.nav - 100.0).abs() < 0.01);
}

/// Buy on Friday -> Saturday/Sunday get Friday's price via forward-fill.
#[tokio::test]
async fn test_weekend_forward_fill() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    // 2025-01-03 is a Friday
    common::insert_transaction(&db, asset_id, "2025-01-03", 10.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 50.0, false).await;
    // No prices for Sat/Sun -- forward-fill should kick in

    let start = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap_fri = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    let snap_sat = common::get_portfolio_snapshot(&db, "2025-01-04")
        .await
        .unwrap();
    let snap_sun = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .unwrap();

    // All should have same NAV = 100 (forward-fill from Friday's price)
    assert!((snap_fri.nav - 100.0).abs() < 0.01);
    assert!((snap_sat.nav - 100.0).abs() < 0.01);
    assert!((snap_sun.nav - 100.0).abs() < 0.01);
    // Same outstanding shares
    assert_eq!(snap_fri.outstanding_shares, snap_sat.outstanding_shares);
    assert_eq!(snap_fri.outstanding_shares, snap_sun.outstanding_shares);
}

/// Full history exists, rebuild from mid-date -> only recalculates from that date;
/// earlier history unchanged.
#[tokio::test]
async fn test_rebuild_from_specific_date() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    for (date, price) in [
        ("2025-01-02", 50.0),
        ("2025-01-03", 55.0),
        ("2025-01-04", 60.0),
        ("2025-01-05", 65.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    // Build full history
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap_d2_before = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();

    // Rebuild from day 4 only (incremental, using day 3 snapshot as prev)
    let prev_snap = portfolio_history_repo::find_at_or_before(&db, "2025-01-03")
        .await
        .unwrap();
    let start_d4 = NaiveDate::from_ymd_opt(2025, 1, 4).unwrap();
    nav::rebuild_portfolio_history(&db, start_d4, prev_snap.as_ref(), &mock)
        .await
        .unwrap();

    let snap_d2_after = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    let snap_d3_after = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();

    // Day 2 and 3 should be unchanged
    assert_eq!(snap_d2_before.nav, snap_d2_after.nav);
    assert_eq!(
        snap_d2_before.outstanding_shares,
        snap_d2_after.outstanding_shares
    );

    // Day 4 and 5 should exist with correct values
    let snap_d4 = common::get_portfolio_snapshot(&db, "2025-01-04")
        .await
        .unwrap();
    // 10 shares @ 60 = 600, 5 outstanding, NAV = 120
    assert!((snap_d4.asset_value - 600.0).abs() < 0.01);
    assert!((snap_d4.nav - 120.0).abs() < 0.01);

    // Day 3 should still be there
    assert!((snap_d3_after.nav - 110.0).abs() < 0.01);
}

/// History built up to day 10, insert buy on day 3, rebuild from day 3
/// -> correctly recomputes days 3 onwards.
#[tokio::test]
async fn test_back_dated_buy() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;

    // Prices for 5 days (all at 50)
    for day in 2..=6 {
        let date = format!("2025-01-{:02}", day);
        common::insert_daily_price(&db, asset_id, &date, 50.0, false).await;
    }

    // Build initial history
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap_d5_before = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .unwrap();
    assert_eq!(snap_d5_before.outstanding_shares, 5.0); // 500/100

    // Add a back-dated buy on day 3
    common::insert_transaction(&db, asset_id, "2025-01-03", 10.0, 50.0, 0.0).await;

    // Rebuild from day 3 (incremental, using day 2 snapshot as prev)
    let prev_snap = portfolio_history_repo::find_at_or_before(&db, "2025-01-02")
        .await
        .unwrap();
    let start_d3 = NaiveDate::from_ymd_opt(2025, 1, 3).unwrap();
    nav::rebuild_portfolio_history(&db, start_d3, prev_snap.as_ref(), &mock)
        .await
        .unwrap();

    let snap_d5_after = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .unwrap();

    // Now outstanding_shares should be higher: first buy 5 + second buy 5 = 10
    assert!((snap_d5_after.outstanding_shares - 10.0).abs() < 0.01);
    // Holdings: 20 shares @ 50 = 1000, NAV = 1000/10 = 100
    assert!((snap_d5_after.asset_value - 1000.0).abs() < 0.01);
    assert!((snap_d5_after.nav - 100.0).abs() < 0.01);
}

/// Two assets with different prices -> asset_value = sum of both.
#[tokio::test]
async fn test_multiple_assets() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_a =
        common::insert_asset(&db, "XFAKE1", "Asset A", "stock", None, "EUR").await;
    let asset_b =
        common::insert_asset(&db, "XFAKE2", "Asset B", "stock", None, "EUR").await;

    // Buy 10 of A @ $50 and 5 of B @ $100
    common::insert_transaction(&db, asset_a, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_transaction(&db, asset_b, "2025-01-02", 5.0, 100.0, 0.0).await;

    common::insert_daily_price(&db, asset_a, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_b, "2025-01-02", 100.0, false).await;

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();

    // Total deposit = 500 + 500 = 1000
    // First buy: NAV=100, shares=5 (500/100)
    // Second buy: NAV still 100, shares = 500/100 = 5
    // Total outstanding = 10
    // EOD: asset_value = 10*50 + 5*100 = 1000
    // NAV = 1000 / 10 = 100
    assert!((snap.outstanding_shares - 10.0).abs() < 0.01);
    assert!((snap.nav - 100.0).abs() < 0.01);
    assert!((snap.asset_value - 1000.0).abs() < 0.01);
}

/// Asset has no cached price -> asset skipped in valuation.
#[tokio::test]
async fn test_missing_price_for_asset() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    // Deliberately NOT inserting any daily price

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();

    // No price -> asset_value = 0, but outstanding_shares still set from deposit
    assert_eq!(snap.asset_value, 0.0);
    assert_eq!(snap.outstanding_shares, 5.0); // 500/100
    // NAV = 0/5 = 0
    assert_eq!(snap.nav, 0.0);
}

// ========== NEW TESTS ==========

/// After rebuild, portfolio_asset_history rows exist with correct values.
#[tokio::test]
async fn test_per_asset_history_created() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 55.0, false).await;

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let asset_snaps_d1 = common::get_asset_snapshots(&db, "2025-01-02").await;
    assert_eq!(asset_snaps_d1.len(), 1);
    assert_eq!(asset_snaps_d1[0].asset_id, asset_id);
    assert!((asset_snaps_d1[0].quantity - 10.0).abs() < 0.01);
    assert!((asset_snaps_d1[0].closing_price - 50.0).abs() < 0.01);
    assert!((asset_snaps_d1[0].market_value - 500.0).abs() < 0.01);

    let asset_snaps_d2 = common::get_asset_snapshots(&db, "2025-01-03").await;
    assert_eq!(asset_snaps_d2.len(), 1);
    assert!((asset_snaps_d2[0].closing_price - 55.0).abs() < 0.01);
    assert!((asset_snaps_d2[0].market_value - 550.0).abs() < 0.01);
}

/// Two assets -> each has its own row per day in portfolio_asset_history.
#[tokio::test]
async fn test_per_asset_history_multiple_assets() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_a =
        common::insert_asset(&db, "XFAKE1", "Asset A", "stock", None, "EUR").await;
    let asset_b =
        common::insert_asset(&db, "XFAKE2", "Asset B", "stock", None, "EUR").await;

    common::insert_transaction(&db, asset_a, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_transaction(&db, asset_b, "2025-01-02", 5.0, 100.0, 0.0).await;

    common::insert_daily_price(&db, asset_a, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_b, "2025-01-02", 100.0, false).await;

    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    let asset_snaps = common::get_asset_snapshots(&db, "2025-01-02").await;
    assert_eq!(asset_snaps.len(), 2);

    // Sorted by asset_id, so asset_a (id=1) first, asset_b (id=2) second
    let snap_a = &asset_snaps[0];
    let snap_b = &asset_snaps[1];

    assert_eq!(snap_a.asset_id, asset_a);
    assert!((snap_a.quantity - 10.0).abs() < 0.01);
    assert!((snap_a.closing_price - 50.0).abs() < 0.01);
    assert!((snap_a.market_value - 500.0).abs() < 0.01);

    assert_eq!(snap_b.asset_id, asset_b);
    assert!((snap_b.quantity - 5.0).abs() < 0.01);
    assert!((snap_b.closing_price - 100.0).abs() < 0.01);
    assert!((snap_b.market_value - 500.0).abs() < 0.01);
}

/// Unit test for the pure process_day_transactions function (no DB).
#[tokio::test]
async fn test_process_day_transactions_pure() {
    // Simulate first buy: 10 shares @ $50
    let tx1 = Transaction {
        asset_id: 1,
        date: "2025-01-02".to_owned(),
        quantity: 10.0,
        price_cents: 5000,
        fees_cents: 0,
    };

    let mut holdings: HashMap<i32, f64> = HashMap::new();
    let txs: Vec<&Transaction> = vec![&tx1];

    let (os, nav_val) = nav::process_day_transactions(&txs, &mut holdings, 0.0, 100.0);

    // First buy: deposit=500, NAV=100, shares=5
    assert!((os - 5.0).abs() < 0.01);
    assert!((nav_val - 100.0).abs() < 0.01);
    assert_eq!(*holdings.get(&1).unwrap(), 10.0);

    // Simulate second buy at NAV=100
    let tx2 = Transaction {
        asset_id: 1,
        date: "2025-01-03".to_owned(),
        quantity: 5.0,
        price_cents: 6000,
        fees_cents: 0,
    };

    let txs2: Vec<&Transaction> = vec![&tx2];
    let (os2, nav_val2) = nav::process_day_transactions(&txs2, &mut holdings, os, nav_val);

    // Second buy: deposit=300, shares_issued=300/100=3, outstanding=5+3=8
    assert!((os2 - 8.0).abs() < 0.01);
    assert!((nav_val2 - 100.0).abs() < 0.01);
    assert_eq!(*holdings.get(&1).unwrap(), 15.0);
}

/// Insert transaction directly (no rebuild) -> portfolio_history is empty.
/// Then call rebuild -> portfolio_history is populated.
#[tokio::test]
async fn test_lazy_rebuild_no_history_on_buy() {
    let db = common::setup_test_db().await;
    let mock = common::MockPriceFetcher::new();

    let asset_id =
        common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", None, "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;

    // No rebuild called yet -> portfolio_history should be empty
    let snapshots = common::get_all_snapshots(&db).await;
    assert!(snapshots.is_empty());

    // Now trigger rebuild
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(&db, start, None, &mock)
        .await
        .unwrap();

    // portfolio_history should now have rows
    let snapshots = common::get_all_snapshots(&db).await;
    assert!(!snapshots.is_empty());

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    assert!((snap.nav - 100.0).abs() < 0.01);
}
