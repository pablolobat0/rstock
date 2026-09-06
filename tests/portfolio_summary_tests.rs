#![allow(clippy::float_cmp)]

pub mod common;

use chrono::NaiveDate;
use rstock::db::entities::portfolio_history;
use rstock::db::repos::portfolio_history_repo;
use rstock::services::metrics::compute_cagr;
use rstock::services::portfolio;
use sea_orm::{EntityTrait, Set};
use serde_json::json;

fn fixed_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()
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

/// No `portfolio_history` -> `find_latest` returns None.
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
    let price_date = "2025-06-03";
    let asset_id = common::insert_fund_asset(&db, "XFAKEF1", "Fake Fund", "EUR", "F000FAKE").await;
    common::insert_transaction(&db, asset_id, price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, price_date, 100.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();

    assert!(result.market_data_limitations.is_empty());
}

#[tokio::test]
async fn test_portfolio_surfaces_excessive_morningstar_lag_warning() {
    let db = common::setup_test_db().await;
    let price_date = "2025-06-01";
    let asset_id =
        common::insert_fund_asset(&db, "XFAKEF2", "Delayed Fund", "EUR", "F000DELAY").await;
    common::insert_transaction(&db, asset_id, price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, price_date, 100.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_current_positions(&db, &market_data)
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
    let price_date = "2025-06-01";
    let asset_id = common::insert_asset(&db, "XFAKES1", "Stale Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, price_date, 100.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_current_positions(&db, &market_data)
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
    let stale_fx_date = "2025-06-01";
    let fresh_price_date = "2025-06-09";
    let asset_id = common::insert_asset(&db, "XFAKES2", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, stale_fx_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, stale_fx_date, 100.0, false).await;
    common::insert_daily_price(&db, asset_id, fresh_price_date, 110.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", stale_fx_date, 0.90).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_current_positions(&db, &market_data)
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

#[tokio::test]
async fn test_portfolio_includes_holding_bought_after_effective_valuation_date() {
    let db = common::setup_test_db().await;
    let stale_date = "2025-06-02";
    let purchase_date = "2025-06-08";
    let split_date = "2025-06-09";
    let stale_fund_id =
        common::insert_fund_asset(&db, "XFAKEF3", "Lagging Fund", "EUR", "F000LAG").await;
    let new_stock_id = common::insert_asset(&db, "XFAKES4", "New Stock", "stock", "EUR").await;

    common::insert_transaction(&db, stale_fund_id, stale_date, 5.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, stale_fund_id, stale_date, 101.0, false).await;
    common::insert_portfolio_snapshot(&db, stale_date, 100.0, 5.0).await;
    common::insert_portfolio_asset_snapshot(&db, stale_date, stale_fund_id, 5.0, 101.0, 505.0, 1.0)
        .await;

    common::insert_transaction(&db, new_stock_id, purchase_date, 3.0, 20.0, 1.0).await;
    common::insert_split_transaction(&db, new_stock_id, split_date, 2.0).await;
    common::insert_daily_price(&db, new_stock_id, split_date, 11.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert_eq!(result.snapshot_date.as_deref(), Some(stale_date));
    assert_eq!(result.rows.len(), 2);
    let position = result
        .rows
        .iter()
        .find(|position| position.ticker == "XFAKES4")
        .unwrap();
    assert!((position.total_qty - 6.0).abs() < 1e-9);
    assert!((position.avg_cost.unwrap() - (61.0 / 6.0)).abs() < 1e-9);
    assert!((position.total_invested.unwrap() - 61.0).abs() < 1e-9);
    assert!((position.current_price.unwrap() - 11.0).abs() < 1e-9);
    assert!((position.current_value.unwrap() - 66.0).abs() < 1e-9);
}

#[tokio::test]
async fn test_unpriced_post_snapshot_holding_remains_visible_with_unavailable_facts() {
    let db = common::setup_test_db().await;
    let snapshot_date = "2025-06-09";
    let priced_stock_id =
        common::insert_asset(&db, "XFAKES5", "Priced Stock", "stock", "EUR").await;
    let unpriced_stock_id =
        common::insert_asset(&db, "XFAKES6", "Unpriced Stock", "stock", "EUR").await;

    common::insert_transaction(&db, priced_stock_id, snapshot_date, 2.0, 10.0, 0.0).await;
    common::insert_daily_price(&db, priced_stock_id, snapshot_date, 11.0, false).await;
    common::insert_portfolio_snapshot(&db, snapshot_date, 100.0, 2.0).await;
    common::insert_portfolio_asset_snapshot(
        &db,
        snapshot_date,
        priced_stock_id,
        2.0,
        11.0,
        22.0,
        1.0,
    )
    .await;
    common::insert_transaction(&db, unpriced_stock_id, "2025-06-10", 1.0, 5.0, 0.0).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert_eq!(result.rows.len(), 2);
    let unpriced = result
        .rows
        .iter()
        .find(|position| position.ticker == "XFAKES6")
        .unwrap();
    assert_eq!(unpriced.total_qty, 1.0);
    assert_eq!(unpriced.total_invested, Some(5.0));
    assert!(unpriced.current_price.is_none());
    assert!(unpriced.price_date.is_none());
    assert!(unpriced.current_value.is_none());
    assert!(unpriced.open_position_gain_loss.is_none());
    assert!(result.total_current_value.is_none());
    assert!(result.total_value.is_none());
    assert_eq!(result.current_position_market_data_limitations.len(), 1);
    let json = serde_json::to_value(&result).unwrap();
    assert!(json["positions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|position| position["ticker"] == "XFAKES6")
        .unwrap()["current_value"]
        .is_null());
}

#[tokio::test]
async fn test_unpriced_holding_without_snapshots_keeps_current_positions_and_scoped_nav_limitations(
) {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEUNPR1", "Unpriced Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-02", 2.0, 10.0, 1.0).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
    assert!(result.nav.is_none());
    assert!(result.snapshot_date.is_none());
    assert_eq!(result.rows.len(), 1);
    let position = &result.rows[0];
    assert_eq!(position.ticker, "XFAKEUNPR1");
    assert_eq!(position.total_qty, 2.0);
    assert_eq!(position.total_invested, Some(21.0));
    assert_eq!(position.avg_cost, Some(10.5));
    assert!(position.current_value.is_none());
    assert!(position.open_position_gain_loss.is_none());
    assert!(result.total_current_value.is_none());
    // The unpriced holding limits NAV in the NAV scope...
    assert!(result.nav_market_data_limitations.iter().any(|limitation| {
        matches!(
            limitation.subject,
            rstock::models::MarketDataSubject::Asset { ref ticker, .. }
                if ticker == "XFAKEUNPR1"
        )
    }));
    // ...and independently in the current-position scope, while Monetary is clear.
    assert!(result
        .current_position_market_data_limitations
        .iter()
        .any(|limitation| {
            matches!(
                limitation.subject,
                rstock::models::MarketDataSubject::Asset { ref ticker, .. }
                    if ticker == "XFAKEUNPR1"
            )
        }));
    assert!(result.monetary_market_data_limitations.is_empty());
}

#[tokio::test]
async fn test_portfolio_returns_monetary_only_holdings_separately() {
    let db = common::setup_test_db().await;
    let price_date = "2025-06-08";
    let asset_id =
        common::insert_monetary_fund_asset(&db, "XFAKEM1", "Monetary Fund", "EUR", "F000MONEY")
            .await;
    common::insert_transaction(&db, asset_id, price_date, 20.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, price_date, 101.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert!(result.rows.is_empty());
    assert_eq!(result.monetary_positions.len(), 1);
    let position = &result.monetary_positions[0];
    assert_eq!(position.ticker, "XFAKEM1");
    assert!((position.total_qty - 20.0).abs() < 1e-9);
    assert!((position.current_price.unwrap() - 101.0).abs() < 1e-9);
    assert!((position.current_value.unwrap() - 2020.0).abs() < 1e-9);
    assert!((position.open_position_gain_loss.unwrap() - 20.0).abs() < 1e-9);
    assert!((result.total_monetary_value.unwrap() - 2020.0).abs() < 1e-9);
    assert!(result.nav.is_none());
    assert!(result.nav_market_data_limitations.is_empty());
    assert!(result.monetary_market_data_limitations.is_empty());
}

#[tokio::test]
async fn test_portfolio_keeps_monetary_holding_when_price_is_missing() {
    let db = common::setup_test_db().await;
    let transaction_date = "2025-06-08";
    let asset_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKEM2",
        "Unpriced Monetary Fund",
        "EUR",
        "F000NOPRICE",
    )
    .await;
    common::insert_transaction(&db, asset_id, transaction_date, 10.0, 50.0, 0.0).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert_eq!(result.monetary_positions.len(), 1);
    let position = &result.monetary_positions[0];
    assert!((position.total_qty - 10.0).abs() < 1e-9);
    assert!((position.avg_cost.unwrap() - 50.0).abs() < 1e-9);
    assert!((position.total_invested.unwrap() - 500.0).abs() < 1e-9);
    assert!(position.current_price.is_none());
    assert!(position.price_date.is_none());
    assert!(position.current_value.is_none());
    assert!(position.open_position_gain_loss.is_none());
    assert!(position.open_position_gain_loss_pct.is_none());
    assert_eq!(position.market_data_limitations.len(), 1);
    assert!(result.total_monetary_value.is_none());
    assert!(result.nav_market_data_limitations.is_empty());
    assert_eq!(result.monetary_market_data_limitations.len(), 1);
}

#[tokio::test]
async fn test_portfolio_excludes_future_monetary_transactions() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKEM3",
        "Future Monetary Fund",
        "EUR",
        "F000FUTURE",
    )
    .await;
    common::insert_transaction(&db, asset_id, "2025-06-11", 10.0, 50.0, 0.0).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert!(result.monetary_positions.is_empty());
    assert!(result.total_monetary_value.unwrap().abs() < 1e-9);
}

#[tokio::test]
async fn test_monetary_cost_basis_accounts_for_splits() {
    let db = common::setup_test_db().await;
    let transaction_date = "2025-06-07";
    let split_date = "2025-06-08";
    let asset_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKEM4",
        "Split Monetary Fund",
        "EUR",
        "F000SPLIT",
    )
    .await;
    common::insert_transaction(&db, asset_id, transaction_date, 10.0, 100.0, 0.0).await;
    common::insert_split_transaction(&db, asset_id, split_date, 2.0).await;
    common::insert_daily_price(&db, asset_id, split_date, 60.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();
    let position = &result.monetary_positions[0];

    assert!((position.total_qty - 20.0).abs() < 1e-9);
    assert!((position.avg_cost.unwrap() - 50.0).abs() < 1e-9);
    assert!((position.total_invested.unwrap() - 1000.0).abs() < 1e-9);
    assert!((position.open_position_gain_loss.unwrap() - 200.0).abs() < 1e-9);
}

#[tokio::test]
async fn test_monetary_snapshot_is_not_returned_as_performance_position() {
    let db = common::setup_test_db().await;
    let snapshot_date = "2025-06-09";
    let stock_id = common::insert_asset(&db, "XFAKES3", "Performance Stock", "stock", "EUR").await;
    let monetary_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKEM5",
        "Legacy Monetary Fund",
        "EUR",
        "F000LEGACY",
    )
    .await;
    common::insert_transaction(&db, stock_id, snapshot_date, 10.0, 100.0, 0.0).await;
    common::insert_transaction(&db, monetary_id, snapshot_date, 5.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, stock_id, snapshot_date, 110.0, false).await;
    common::insert_daily_price(&db, monetary_id, snapshot_date, 101.0, false).await;
    common::insert_portfolio_snapshot(&db, snapshot_date, 100.0, 10.0).await;
    common::insert_portfolio_asset_snapshot(&db, snapshot_date, stock_id, 10.0, 110.0, 1100.0, 1.0)
        .await;
    common::insert_portfolio_asset_snapshot(
        &db,
        snapshot_date,
        monetary_id,
        5.0,
        101.0,
        505.0,
        1.0,
    )
    .await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].ticker, "XFAKES3");
    assert_eq!(result.monetary_positions.len(), 1);
    assert_eq!(result.monetary_positions[0].ticker, "XFAKEM5");
    assert!((result.total_invested.unwrap() - 1000.0).abs() < 1e-9);
    assert!((result.total_current_value.unwrap() - 1100.0).abs() < 1e-9);
    assert!((result.total_open_position_gain_loss.unwrap() - 100.0).abs() < 1e-9);
    assert!((result.nav.unwrap() - 100.0).abs() < 1e-9);
}

#[tokio::test]
async fn portfolio_view_applies_open_position_facts_equally_to_performance_and_monetary_holdings() {
    let db = common::setup_test_db().await;
    let stock_id =
        common::insert_asset(&db, "XFAKEFACT1", "Performance Stock", "stock", "EUR").await;
    let monetary_id =
        common::insert_monetary_fund_asset(&db, "XFAKEFACT2", "Monetary Fund", "EUR", "F000FACTS")
            .await;
    for asset_id in [stock_id, monetary_id] {
        common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 1.0).await;
        common::insert_transaction(&db, asset_id, "2025-06-02", 4.0, 14.0, 2.0).await;
        common::insert_split_transaction(&db, asset_id, "2025-06-03", 2.0).await;
        common::insert_dividend_transaction(&db, asset_id, "2025-06-04", 10.0, 1.0).await;
        common::insert_sell_transaction(&db, asset_id, "2025-06-05", 3.0, 20.0, 0.0).await;
    }
    common::insert_portfolio_snapshot(&db, "2025-06-09", 100.0, 1.0).await;
    common::insert_portfolio_asset_snapshot(&db, "2025-06-09", stock_id, 9.0, 8.0, 72.0, 1.0).await;

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKEFACT1".to_owned(),
        vec![("2025-06-09".to_owned(), 8.0)],
    );
    sources
        .historical_prices
        .insert("F000FACTS".to_owned(), vec![("2025-06-09".to_owned(), 8.0)]);
    let market_data = common::market_data_at(
        &sources,
        chrono::NaiveDate::from_ymd_opt(2025, 6, 10).unwrap(),
    );

    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();
    let performance = &result.rows[0];
    let monetary = &result.monetary_positions[0];

    assert!((performance.total_qty - 9.0).abs() < 1e-9);
    assert!((performance.total_invested.unwrap() - 59.25).abs() < 1e-9);
    assert!((performance.avg_cost.unwrap() - 6.583_333_333_3).abs() < 1e-9);
    assert!((performance.dividends_received.unwrap() - 9.0).abs() < 1e-9);
    assert!((performance.open_position_gain_loss.unwrap() - 12.75).abs() < 1e-9);
    assert_eq!(performance.total_invested, monetary.total_invested);
    assert_eq!(performance.avg_cost, monetary.avg_cost);
    assert_eq!(performance.dividends_received, monetary.dividends_received);
    assert_eq!(
        performance.open_position_gain_loss,
        monetary.open_position_gain_loss
    );

    let json_output = serde_json::to_value(&result).unwrap();
    assert_eq!(
        json_output["positions"][0]["dividends_received"],
        json!(9.0)
    );
    assert_eq!(
        json_output["positions"][0]["open_position_gain_loss"],
        json!(12.75)
    );
    assert_eq!(
        json_output["monetary_positions"][0]["dividends_received"],
        json!(9.0)
    );
    assert_eq!(
        json_output["monetary_positions"][0]["open_position_gain_loss"],
        json!(12.75)
    );
}

#[tokio::test]
async fn nav_limitations_ignore_closed_assets_and_start_at_the_open_holding_period() {
    let db = common::setup_test_db().await;
    let closed_id = common::insert_asset(&db, "XFAKECLOSED", "Closed", "stock", "EUR").await;
    let open_id = common::insert_asset(&db, "XFAKEOPEN", "Open", "stock", "EUR").await;
    common::insert_transaction(&db, closed_id, "2025-01-01", 1.0, 10.0, 0.0).await;
    common::insert_sell_transaction(&db, closed_id, "2025-01-02", 1.0, 10.0, 0.0).await;
    common::insert_transaction(&db, open_id, "2025-01-03", 1.0, 10.0, 0.0).await;
    for date in ["2025-01-01", "2025-01-02", "2025-01-03", "2025-01-04"] {
        common::insert_daily_price(&db, closed_id, date, 10.0, false).await;
    }

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKEOPEN".to_owned(),
        vec![
            ("2025-01-03".to_owned(), 10.0),
            ("2025-01-04".to_owned(), 11.0),
        ],
    );
    sources.historical_prices.insert(
        "ACWI".to_owned(),
        vec![
            ("2025-01-01".to_owned(), 100.0),
            ("2025-01-02".to_owned(), 100.0),
            ("2025-01-03".to_owned(), 100.0),
            ("2025-01-04".to_owned(), 100.0),
        ],
    );
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        vec![
            ("2025-01-01".to_owned(), 0.9),
            ("2025-01-02".to_owned(), 0.9),
            ("2025-01-03".to_owned(), 0.9),
            ("2025-01-04".to_owned(), 0.9),
        ],
    );
    let market_data = common::market_data_at(
        &sources,
        chrono::NaiveDate::from_ymd_opt(2025, 1, 5).unwrap(),
    );

    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();
    assert!(result.nav_market_data_limitations.is_empty());
}
