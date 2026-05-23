mod common;

use chrono::NaiveDate;
use rstock::db::repos::portfolio_history_repo;
use rstock::services::nav;

/// Full flow: insert asset + 2 transactions + daily prices -> rebuild ->
/// verify portfolio_history rows and values for specific dates.
/// Also verifies portfolio_asset_history rows are created.
#[tokio::test]
async fn test_full_buy_rebuild_summary_flow() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Fake Corp", "stock", "EUR").await;

    // Buy 1: 10 shares @ 150 on Jan 2 with 5 fees
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 150.0, 5.0).await;
    // Buy 2: 5 shares @ $160 on Jan 6 with $5 fees
    common::insert_transaction(&db, asset_id, "2025-01-06", 5.0, 160.0, 5.0).await;

    // After inserting transactions (no rebuild), portfolio_history should be empty
    let snapshots = common::get_all_snapshots(&db).await;
    assert!(
        snapshots.is_empty(),
        "buy should not create portfolio_history rows"
    );

    // Daily prices
    for (date, price) in [
        ("2025-01-02", 150.0),
        ("2025-01-03", 152.0),
        ("2025-01-04", 152.0),
        ("2025-01-05", 152.0),
        ("2025-01-06", 160.0),
        ("2025-01-07", 165.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    // Trigger rebuild (simulating what get_portfolio_summary does)
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(
        &db,
        start,
        NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
        None,
        &common::market_data(&mock),
    )
    .await
    .unwrap();

    // Day 1 (Jan 2): deposit = 10*150 + 5 = 1505, NAV=100, shares=15.05
    // EOD: 10*150=1500, NAV = 1500/15.05 ~ 99.67
    let s0 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .expect("Jan 2 snapshot");
    assert_eq!(s0.outstanding_shares, 1505.0 / 100.0);
    assert!((s0.asset_value - 1500.0).abs() < 0.01);

    // Verify per-asset history for Jan 2
    let asset_snaps = common::get_asset_snapshots(&db, "2025-01-02").await;
    assert_eq!(asset_snaps.len(), 1);
    assert_eq!(asset_snaps[0].asset_id, asset_id);
    assert!((asset_snaps[0].quantity - 10.0).abs() < 0.01);
    assert!((asset_snaps[0].closing_price - 150.0).abs() < 0.01);
    assert!((asset_snaps[0].market_value - 1500.0).abs() < 0.01);

    // Day 5 (Jan 6): price=160, buy 5@160+5=805. NAV from prev day. shares_issued = 805/nav.
    // After buy: holdings=15, outstanding=15.05+shares_issued
    // EOD: 15*160=2400
    let s4 = common::get_portfolio_snapshot(&db, "2025-01-06")
        .await
        .expect("Jan 6 snapshot");
    assert!((s4.asset_value - 2400.0).abs() < 0.01);
    // Outstanding shares should have increased from the second buy
    let s3 = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .expect("Jan 5 snapshot");
    assert!(s4.outstanding_shares > s3.outstanding_shares);

    // Day 6 (Jan 7): price = 165, no new transactions
    let s5 = common::get_portfolio_snapshot(&db, "2025-01-07")
        .await
        .expect("Jan 7 snapshot");
    assert!((s5.asset_value - (15.0 * 165.0)).abs() < 0.01);
    assert_eq!(s5.outstanding_shares, s4.outstanding_shares);

    // Verify per-asset history for Jan 7
    let asset_snaps_d7 = common::get_asset_snapshots(&db, "2025-01-07").await;
    assert_eq!(asset_snaps_d7.len(), 1);
    assert!((asset_snaps_d7[0].quantity - 15.0).abs() < 0.01);
    assert!((asset_snaps_d7[0].closing_price - 165.0).abs() < 0.01);
}

/// Build history for buy 1, add buy 2, rebuild from buy 2 date ->
/// verify outstanding_shares increased, NAV didn't jump.
#[tokio::test]
async fn test_incremental_rebuild_after_second_buy() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Fake Corp", "stock", "EUR").await;

    // Initial buy: 20 shares @ 100
    common::insert_transaction(&db, asset_id, "2025-01-02", 20.0, 100.0, 0.0).await;

    for (date, price) in [
        ("2025-01-02", 100.0),
        ("2025-01-03", 105.0),
        ("2025-01-04", 105.0),
        ("2025-01-05", 105.0),
        ("2025-01-06", 110.0),
        ("2025-01-07", 115.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    // Build initial history
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    nav::rebuild_portfolio_history(
        &db,
        start,
        NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
        None,
        &common::market_data(&mock),
    )
    .await
    .unwrap();

    let snap_d5_before = common::get_portfolio_snapshot(&db, "2025-01-06")
        .await
        .unwrap();

    // Before second buy: 20 shares @ 110 = 2200, outstanding = 20, NAV = 110
    assert!((snap_d5_before.asset_value - 2200.0).abs() < 0.01);
    assert!((snap_d5_before.nav - 110.0).abs() < 0.01);

    // Add second buy on Jan 6: 10 shares @ $110
    common::insert_transaction(&db, asset_id, "2025-01-06", 10.0, 110.0, 0.0).await;

    // Rebuild from Jan 6 (incremental, using Jan 5 snapshot as prev)
    let prev_snap = portfolio_history_repo::find_at_or_before(&db, "2025-01-05")
        .await
        .unwrap();
    let start_d6 = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
    nav::rebuild_portfolio_history(
        &db,
        start_d6,
        NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
        prev_snap.as_ref(),
        &common::market_data(&mock),
    )
    .await
    .unwrap();

    let snap_d5_after = common::get_portfolio_snapshot(&db, "2025-01-06")
        .await
        .unwrap();

    // Outstanding shares should have increased
    assert!(snap_d5_after.outstanding_shares > snap_d5_before.outstanding_shares);

    // The NAV engine uses Jan 5 EOD NAV for share issuance on Jan 6.
    // Jan 5 EOD: 20 * 105 = 2100, outstanding = 20, NAV = 105
    // Second buy: deposit = 10*110 = 1100, shares_issued = 1100/105 ~ 10.476
    // Total outstanding ~ 30.476, holdings = 30
    // EOD: 30 * 110 = 3300, NAV = 3300 / 30.476 ~ 108.28
    let expected_shares_issued = 1100.0 / 105.0;
    let expected_outstanding = 20.0 + expected_shares_issued;
    let expected_nav = 3300.0 / expected_outstanding;

    assert!((snap_d5_after.outstanding_shares - expected_outstanding).abs() < 0.01);
    assert!((snap_d5_after.asset_value - 3300.0).abs() < 0.01);
    assert!((snap_d5_after.nav - expected_nav).abs() < 0.01);

    // Day after should also be correct
    let snap_d6 = common::get_portfolio_snapshot(&db, "2025-01-07")
        .await
        .unwrap();
    assert!((snap_d6.asset_value - (30.0 * 115.0)).abs() < 0.01);
    assert_eq!(snap_d6.outstanding_shares, snap_d5_after.outstanding_shares);
}
