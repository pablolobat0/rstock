mod common;

use chrono::Duration;
use rstock::db::entities::portfolio_history;
use rstock::db::repos::portfolio_history_repo;
use rstock::services::metrics::compute_cagr;
use rstock::services::portfolio;
use sea_orm::{EntityTrait, Set};

fn date_string(days_before_yesterday: i64) -> String {
    let yesterday = chrono::Local::now().date_naive() - Duration::days(1);
    rstock::constants::format_date(yesterday - Duration::days(days_before_yesterday))
}

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

/// No portfolio_history -> find_latest returns None.
#[tokio::test]
async fn test_returns_none_when_no_history() {
    let db = common::setup_test_db().await;

    let snapshot = portfolio_history_repo::find_latest(&db).await.unwrap();
    assert!(snapshot.is_none());
}

/// Snapshot at Jan 1 with NAV=100, later snapshot with NAV=110 -> return is +10%.
#[tokio::test]
async fn test_ytd_return_calculation() {
    let db = common::setup_test_db().await;

    insert_snapshot(&db, "2025-01-01", 100.0, 500.0, 5.0).await;
    insert_snapshot(&db, "2025-06-15", 110.0, 550.0, 5.0).await;

    // Use find_at_or_before to get the reference point
    let base = portfolio_history_repo::find_at_or_before(&db, "2025-01-01")
        .await
        .unwrap()
        .unwrap();
    let current = portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .unwrap();

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
    let snapshot = portfolio_history_repo::find_at_or_before(&db, "2024-01-01")
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

    let base = portfolio_history_repo::find_at_or_before(&db, "2025-01-01")
        .await
        .unwrap()
        .unwrap();
    let current = portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .unwrap();

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

    let snap = portfolio_history_repo::find_at_or_before(&db, "2025-01-07")
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
    let cagr = compute_cagr("2023-01-01", "2026-01-01", start_nav, end_nav).unwrap();
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
    let cagr = compute_cagr("2021-01-01", "2026-01-01", start_nav, end_nav).unwrap();
    assert!(cagr.abs() < 0.01);
}

/// CAGR with loss -> negative
#[tokio::test]
async fn test_cagr_with_loss() {
    let start_nav = 100.0_f64;
    let end_nav = 50.0_f64;
    let cagr = compute_cagr("2021-01-01", "2026-01-01", start_nav, end_nav).unwrap();
    // 0.5^(1/5) - 1 ~ -0.1294 -> -12.94%
    assert!((cagr - (-12.94)).abs() < 0.1);
}

#[tokio::test]
async fn test_portfolio_suppresses_acceptable_morningstar_lag_warning() {
    let db = common::setup_test_db().await;
    let price_date = date_string(6);
    let asset_id = common::insert_fund_asset(&db, "XFAKEF1", "Fake Fund", "EUR", "F000FAKE").await;
    common::insert_transaction(&db, asset_id, &price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, &price_date, 100.0, false).await;

    let result = portfolio::get_asset_positions(&db, &common::MockPriceFetcher::new())
        .await
        .unwrap();

    assert!(result.market_data_limitations.is_empty());
}

#[tokio::test]
async fn test_portfolio_surfaces_excessive_morningstar_lag_warning() {
    let db = common::setup_test_db().await;
    let price_date = date_string(8);
    let asset_id =
        common::insert_fund_asset(&db, "XFAKEF2", "Delayed Fund", "EUR", "F000DELAY").await;
    common::insert_transaction(&db, asset_id, &price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, &price_date, 100.0, false).await;

    let result = portfolio::get_asset_positions(&db, &common::MockPriceFetcher::new())
        .await
        .unwrap();

    assert_eq!(result.market_data_limitations.len(), 1);
    assert_eq!(
        result.market_data_limitations[0].subject,
        rstock::models::MarketDataSubject::Asset {
            ticker: "XFAKEF2".to_owned(),
            name: "Delayed Fund".to_owned(),
            asset_type: rstock::models::AssetType::Fund,
        }
    );
}

#[tokio::test]
async fn test_portfolio_surfaces_stock_stale_data_warning() {
    let db = common::setup_test_db().await;
    let price_date = date_string(8);
    let asset_id = common::insert_asset(&db, "XFAKES1", "Stale Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, &price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, &price_date, 100.0, false).await;

    let result = portfolio::get_asset_positions(&db, &common::MockPriceFetcher::new())
        .await
        .unwrap();

    assert_eq!(result.market_data_limitations.len(), 1);
    assert_eq!(
        result.market_data_limitations[0].subject,
        rstock::models::MarketDataSubject::Asset {
            ticker: "XFAKES1".to_owned(),
            name: "Stale Stock".to_owned(),
            asset_type: rstock::models::AssetType::Stock,
        }
    );
}

#[tokio::test]
async fn test_portfolio_surfaces_fx_stale_data_warning() {
    let db = common::setup_test_db().await;
    let stale_fx_date = date_string(8);
    let fresh_price_date = date_string(0);
    let asset_id = common::insert_asset(&db, "XFAKES2", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, &stale_fx_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, &stale_fx_date, 100.0, false).await;
    common::insert_daily_price(&db, asset_id, &fresh_price_date, 110.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", &stale_fx_date, 0.90).await;

    let result = portfolio::get_asset_positions(&db, &common::MockPriceFetcher::new())
        .await
        .unwrap();

    assert_eq!(result.market_data_limitations.len(), 1);
    assert_eq!(
        result.market_data_limitations[0].subject,
        rstock::models::MarketDataSubject::FxRate {
            currency: "USD".to_owned(),
        }
    );
}
