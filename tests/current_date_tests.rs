pub mod common;

use chrono::NaiveDate;
use rstock::db::repos::{asset_repo, portfolio_history_repo};
use rstock::services::{analytics, composition, nav, portfolio};

fn fixed_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()
}

#[tokio::test]
async fn fixed_clock_excludes_future_transactions_from_current_inventory() {
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_monetary_fund_asset(&db, "XFAKECLOCK1", "Clock Fund", "EUR", "F000CLOCK")
            .await;
    common::insert_transaction(&db, asset_id, "2025-06-10", 2.0, 100.0, 0.0).await;
    common::insert_transaction(&db, asset_id, "2025-06-11", 3.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 101.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert_eq!(result.monetary_positions.len(), 1);
    assert!((result.monetary_positions[0].total_qty - 2.0).abs() < 1e-9);
}

#[tokio::test]
async fn fixed_clock_excludes_future_sells_and_dividends_from_portfolio_inventory() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKECLOCKSELL",
        "Clock Sell Fund",
        "EUR",
        "F000CLOCKSELL",
    )
    .await;
    common::insert_transaction(&db, asset_id, "2025-06-09", 2.0, 100.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-10", 10.0, 0.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-06-11", 1.0, 100.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-11", 20.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 101.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert_eq!(result.monetary_positions.len(), 1);
    let position = &result.monetary_positions[0];
    assert!((position.total_qty - 2.0).abs() < 1e-9);
    assert_eq!(position.dividends_received, Some(10.0));
}

#[tokio::test]
async fn fixed_clock_limits_historical_market_data_to_latest_completed_date() {
    let db = common::setup_test_db().await;
    common::insert_asset(&db, "XFAKECLOCK2", "Clock Stock", "stock", "EUR").await;
    let asset = asset_repo::find_by_ticker(&db, "XFAKECLOCK2")
        .await
        .unwrap()
        .unwrap();
    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        asset.ticker.clone(),
        vec![
            ("2025-06-09".to_owned(), 100.0),
            ("2025-06-10".to_owned(), 101.0),
        ],
    );

    let market_data = common::market_data_at(&sources, fixed_today());
    let prepared = market_data
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-06-09",
            "2025-06-10",
        )
        .await
        .unwrap();

    assert_eq!(
        prepared.effective_end,
        NaiveDate::from_ymd_opt(2025, 6, 9).unwrap()
    );
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-06-10")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn nav_readiness_rebuilds_through_fixed_clock_cutoff_before_portfolio_view() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKENAV1", "NAV Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-08", 1.0, 100.0, 0.0).await;

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKENAV1".to_owned(),
        vec![
            ("2025-06-08".to_owned(), 100.0),
            ("2025-06-09".to_owned(), 101.0),
            ("2025-06-10".to_owned(), 102.0),
        ],
    );
    let market_data = common::market_data_at(&sources, fixed_today());

    nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    assert_eq!(
        portfolio_history_repo::find_latest(&db)
            .await
            .unwrap()
            .map(|snapshot| snapshot.date),
        Some("2025-06-09".to_owned())
    );
}

#[tokio::test]
async fn first_portfolio_view_keeps_positions_visible_after_rebuilding_nav() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVISIBLE", "Visible Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-08", 2.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-08", 100.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 110.0, false).await;
    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        rstock::constants::BENCHMARK_TICKER.to_owned(),
        vec![
            ("2025-06-08".to_owned(), 200.0),
            ("2025-06-09".to_owned(), 201.0),
        ],
    );
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        vec![
            ("2025-06-08".to_owned(), 0.9),
            ("2025-06-09".to_owned(), 0.9),
        ],
    );
    let market_data = common::market_data_at(&sources, fixed_today());

    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert_eq!(result.rows.len(), 1);
    assert_eq!(result.rows[0].ticker, "XFAKEVISIBLE");
    assert!((result.rows[0].total_qty - 2.0).abs() < 1e-9);
    assert_eq!(result.total_current_value, Some(220.0));
    assert_eq!(result.total_value, Some(220.0));
    assert!(result.nav.is_some());
}

#[tokio::test]
async fn nav_chart_history_is_ready_without_portfolio_view_call_order() {
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_asset(&db, "XFAKENAVCHART", "NAV Chart Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-08", 1.0, 100.0, 0.0).await;

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKENAVCHART".to_owned(),
        vec![
            ("2025-06-08".to_owned(), 100.0),
            ("2025-06-09".to_owned(), 101.0),
        ],
    );
    let market_data = common::market_data_at(&sources, fixed_today());

    nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();
    let snapshots = nav::get_ready_portfolio_history(&db, "2025-06-08", "2025-06-09")
        .await
        .unwrap();

    assert_eq!(
        snapshots.last().map(|snapshot| snapshot.date.as_str()),
        Some("2025-06-09")
    );
}

#[tokio::test]
async fn fixed_clock_future_dated_performance_only_portfolio_is_empty_and_never_errors() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEFUT1", "Future Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-11", 2.0, 10.0, 0.0).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();

    assert!(result.rows.is_empty());
    assert!(result.monetary_positions.is_empty());
    assert!(result.nav.is_none());
    assert!(result.snapshot_date.is_none());
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
    assert!(result.nav_market_data_limitations.is_empty());
    assert!(result.current_position_market_data_limitations.is_empty());
}

#[tokio::test]
async fn composition_does_not_rebuild_nav_history() {
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_asset(&db, "XFAKEPOSITION1", "Current Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-09", 1.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 101.0, false).await;
    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());

    composition::compute_composition(&db, &market_data)
        .await
        .unwrap();

    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn asset_series_correlation_does_not_rebuild_nav_history() {
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_asset(&db, "XFAKECORR1", "Correlation Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 1.0, 100.0, 0.0).await;

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKECORR1".to_owned(),
        vec![
            ("2025-06-01".to_owned(), 100.0),
            ("2025-06-02".to_owned(), 101.0),
        ],
    );
    sources.historical_prices.insert(
        rstock::constants::BENCHMARK_TICKER.to_owned(),
        vec![
            ("2025-06-01".to_owned(), 200.0),
            ("2025-06-02".to_owned(), 201.0),
        ],
    );
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        vec![
            ("2025-06-01".to_owned(), 0.9),
            ("2025-06-02".to_owned(), 0.9),
        ],
    );
    let market_data = common::market_data_at(&sources, fixed_today());

    let matrix = analytics::compute_correlation_data(&db, "2025-06-01", "2025-06-02", &market_data)
        .await
        .unwrap();

    assert!(matrix.names.contains(&"Correlation Stock".to_owned()));
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}
