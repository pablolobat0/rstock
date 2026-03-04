mod common;

use rstock::db::entities::portfolio_history;
use rstock::services::nav;
use sea_orm::{EntityTrait, Set};

async fn insert_snapshot(
    db: &sea_orm::DatabaseConnection,
    date: &str,
    nav: f64,
    total_value: f64,
    outstanding_shares: f64,
) {
    let record = portfolio_history::ActiveModel {
        date: Set(date.to_owned()),
        asset_value: Set(total_value),
        total_value: Set(total_value),
        outstanding_shares: Set(outstanding_shares),
        nav: Set(nav),
    };
    portfolio_history::Entity::insert(record)
        .exec(db)
        .await
        .unwrap();
}

/// No portfolio_history -> get_latest_snapshot returns None.
#[tokio::test]
async fn test_returns_none_when_no_history() {
    let db = common::setup_test_db().await;

    let snapshot = nav::get_latest_snapshot(&db).await.unwrap();
    assert!(snapshot.is_none());
}

/// Snapshot at Jan 1 with NAV=100, later snapshot with NAV=110 -> return is +10%.
#[tokio::test]
async fn test_ytd_return_calculation() {
    let db = common::setup_test_db().await;

    insert_snapshot(&db, "2025-01-01", 100.0, 500.0, 5.0).await;
    insert_snapshot(&db, "2025-06-15", 110.0, 550.0, 5.0).await;

    // Use get_snapshot_at_or_before to get the reference point
    let base = nav::get_snapshot_at_or_before(&db, "2025-01-01")
        .await
        .unwrap()
        .unwrap();
    let current = nav::get_latest_snapshot(&db).await.unwrap().unwrap();

    let return_pct = ((current.nav - base.nav) / base.nav) * 100.0;
    assert!((return_pct - 10.0).abs() < 0.01);
}

/// Portfolio started recently -> no snapshot exists for distant past.
#[tokio::test]
async fn test_return_none_when_period_predates_portfolio() {
    let db = common::setup_test_db().await;

    // Portfolio started in 2025
    insert_snapshot(&db, "2025-06-01", 100.0, 500.0, 5.0).await;

    // Query for 2024 -> None (portfolio didn't exist)
    let snapshot = nav::get_snapshot_at_or_before(&db, "2024-01-01")
        .await
        .unwrap();
    assert!(snapshot.is_none());
}

/// NAV dropped -> returns negative percentage.
#[tokio::test]
async fn test_negative_return() {
    let db = common::setup_test_db().await;

    insert_snapshot(&db, "2025-01-01", 100.0, 500.0, 5.0).await;
    insert_snapshot(&db, "2025-06-15", 85.0, 425.0, 5.0).await;

    let base = nav::get_snapshot_at_or_before(&db, "2025-01-01")
        .await
        .unwrap()
        .unwrap();
    let current = nav::get_latest_snapshot(&db).await.unwrap().unwrap();

    let return_pct = ((current.nav - base.nav) / base.nav) * 100.0;
    assert!((return_pct - (-15.0)).abs() < 0.01);
}

/// Snapshots on Jan 1, Jan 5, Jan 10 -> query Jan 7 returns Jan 5's snapshot.
#[tokio::test]
async fn test_snapshot_at_or_before_finds_closest() {
    let db = common::setup_test_db().await;

    insert_snapshot(&db, "2025-01-01", 100.0, 500.0, 5.0).await;
    insert_snapshot(&db, "2025-01-05", 110.0, 550.0, 5.0).await;
    insert_snapshot(&db, "2025-01-10", 120.0, 600.0, 5.0).await;

    let snap = nav::get_snapshot_at_or_before(&db, "2025-01-07")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(snap.date, "2025-01-05");
    assert!((snap.nav - 110.0).abs() < 0.01);
}

/// CAGR formula: ((end/start)^(1/years) - 1) * 100
/// NAV went from 100 to 200 over 3 years -> CAGR ~ 26.0%
#[tokio::test]
async fn test_cagr_formula() {
    let start_nav = 100.0_f64;
    let end_nav = 200.0_f64;
    let years = 3.0_f64;

    let cagr = ((end_nav / start_nav).powf(1.0 / years) - 1.0) * 100.0;
    // 2^(1/3) - 1 ~ 0.2599 -> 25.99%
    assert!((cagr - 25.99).abs() < 0.1);

    // Simple return would be 100%, but CAGR is ~26%
    let simple = ((end_nav - start_nav) / start_nav) * 100.0;
    assert!((simple - 100.0).abs() < 0.01);
    assert!(cagr < simple);
}

/// CAGR with no growth -> 0%
#[tokio::test]
async fn test_cagr_no_growth() {
    let start_nav = 100.0_f64;
    let end_nav = 100.0_f64;
    let years = 5.0_f64;

    let cagr = ((end_nav / start_nav).powf(1.0 / years) - 1.0) * 100.0;
    assert!(cagr.abs() < 0.01);
}

/// CAGR with loss -> negative
#[tokio::test]
async fn test_cagr_with_loss() {
    let start_nav = 100.0_f64;
    let end_nav = 50.0_f64;
    let years = 5.0_f64;

    let cagr = ((end_nav / start_nav).powf(1.0 / years) - 1.0) * 100.0;
    // 0.5^(1/5) - 1 ~ -0.1294 -> -12.94%
    assert!((cagr - (-12.94)).abs() < 0.1);
}
