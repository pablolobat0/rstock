#![allow(clippy::float_cmp)]

pub mod common;

use chrono::NaiveDate;
use rstock::models::{
    FactAvailability, MarketDataSubject, NavHistoryRequest, PortfolioPerformance,
};
use rstock::services::portfolio_view;

fn fixed_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()
}

async fn empty_view(db: &sea_orm::DatabaseConnection) -> rstock::models::PortfolioView {
    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    portfolio_view::get_portfolio_view(db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap()
}

/// Empty ledger: inventory present with known-zero sections, performance not
/// applicable, and requested history empty.
#[tokio::test]
async fn view_composes_empty_inventory_with_not_applicable_performance() {
    let db = common::setup_test_db().await;
    let view = empty_view(&db).await;

    assert!(view.inventory.performance_holdings.positions.is_empty());
    assert_eq!(
        view.inventory.performance_holdings.aggregates.current_value,
        FactAvailability::Available(0.0)
    );
    assert_eq!(
        view.inventory.monetary_holdings.aggregates.current_value,
        FactAvailability::Available(0.0)
    );
    assert_eq!(view.inventory.total_value, FactAvailability::Available(0.0));
    assert_eq!(view.performance, PortfolioPerformance::NotApplicable);
    assert!(view.nav_history.is_empty());
}

/// Today-dated transactions only: the same expected state as no current or
/// historical measurable performance.
#[tokio::test]
async fn view_composes_future_only_ledger_like_no_measurable_performance() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVFUT", "Future Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-11", 2.0, 100.0, 0.0).await;

    let view = empty_view(&db).await;

    assert_eq!(view.performance, PortfolioPerformance::NotApplicable);
    assert!(view.inventory.performance_holdings.positions.is_empty());
    assert!(view.inventory.monetary_holdings.positions.is_empty());
    assert!(view.nav_history.is_empty());
}

/// Monetary-only holdings still compose the inventory sections, and
/// performance is not applicable because there is nothing to measure.
#[tokio::test]
async fn view_composes_monetary_only_inventory_with_not_applicable_performance() {
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_monetary_fund_asset(&db, "XFAKEVM1", "Money Fund", "EUR", "F000VM1").await;
    common::insert_transaction(&db, asset_id, "2025-06-06", 2.0, 1000.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 1000.0, false).await;

    let view = empty_view(&db).await;

    assert_eq!(view.inventory.monetary_holdings.positions.len(), 1);
    assert_eq!(
        view.inventory.monetary_holdings.positions[0].current_value,
        FactAvailability::Available(2000.0)
    );
    assert_eq!(
        view.inventory.monetary_holdings.aggregates.current_value,
        FactAvailability::Available(2000.0)
    );
    assert_eq!(
        view.inventory.total_value,
        FactAvailability::Available(2000.0)
    );
    assert_eq!(view.performance, PortfolioPerformance::NotApplicable);
    assert!(view.nav_history.is_empty());
}

/// Performance holdings exist but the initial NAV build is blocked by
/// unavailable market data: inventory is preserved and performance becomes
/// unavailable with correctly scoped NAV/history limitations.
#[tokio::test]
async fn view_composes_blocked_initial_nav_as_unavailable_performance() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVB1", "Blocked Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-06", 1.0, 100.0, 0.0).await;
    // No price observations at all: NAV cannot start.

    let sources = common::MockMarketDataSources::new();
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();

    // Inventory stays complete and visible.
    assert_eq!(view.inventory.performance_holdings.positions.len(), 1);
    assert!(matches!(
        view.inventory.performance_holdings.positions[0].current_value,
        FactAvailability::Unavailable
    ));

    let PortfolioPerformance::Unavailable {
        nav_history_limitations,
    } = &view.performance
    else {
        panic!(
            "expected unavailable performance, got {:?}",
            view.performance
        );
    };
    assert!(!nav_history_limitations.is_empty());
    assert!(nav_history_limitations
        .iter()
        .any(|limitation| matches!(limitation.subject, MarketDataSubject::Asset { .. })));
    assert!(view.nav_history.is_empty());
}

/// An older Complete NAV snapshot that cannot advance stays available with
/// its readiness limitations, and the requested history is part of the same
/// successful outcome.
#[tokio::test]
async fn view_keeps_available_performance_at_older_snapshot_with_limitations() {
    let db = common::setup_test_db().await;
    let priced_id = common::insert_asset(&db, "XFAKEVA1", "Priced Stock", "stock", "EUR").await;
    let new_stock_id = common::insert_asset(&db, "XFAKEVA2", "New Stock", "stock", "EUR").await;
    common::insert_transaction(&db, priced_id, "2025-06-02", 2.0, 10.0, 0.0).await;
    common::insert_daily_price(&db, priced_id, "2025-06-06", 11.0, false).await;
    common::insert_portfolio_snapshot(&db, "2025-06-01", 100.0, 0.0).await;
    common::insert_portfolio_snapshot(&db, "2025-06-06", 110.0, 0.2).await;
    common::insert_portfolio_asset_snapshot(&db, "2025-06-06", priced_id, 2.0, 11.0, 22.0, 1.0)
        .await;
    // This post-checkpoint holding has no required price: NAV remains
    // available at its June 6 checkpoint while readiness retains its blocker.
    common::insert_transaction(&db, new_stock_id, "2025-06-07", 1.0, 50.0, 0.0).await;

    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert("XFAKEVA2".to_owned(), vec![("2025-06-09".to_owned(), 50.0)]);
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::OneMonth)
        .await
        .unwrap();

    let PortfolioPerformance::Available(performance) = &view.performance else {
        panic!("expected available performance, got {:?}", view.performance);
    };
    assert_eq!(performance.effective_valuation_date, "2025-06-06");
    // The snapshot helper stores its configured total value of 1,000 EUR.
    assert_eq!(performance.synchronized_value, 1000.0);
    assert_eq!(performance.nav, 110.0);
    assert_eq!(performance.inception_date, "2025-06-01");
    // Limitations must mention the stale holding even though performance is
    // available: a limitation may coexist with an older available snapshot.
    assert!(!performance.nav_history_limitations.is_empty());
    assert!(performance.nav_history_limitations.iter().any(|limitation| {
        matches!(&limitation.subject, MarketDataSubject::Asset { ticker, .. } if ticker == "XFAKEVA2")
    }));

    // Market data prepared for the new holding makes its current inventory
    // value available at the newer date; this mixed-date total stays distinct
    // from the older synchronized NAV value.
    assert_eq!(view.inventory.performance_holdings.positions.len(), 2);
    assert_eq!(
        view.inventory.performance_holdings.aggregates.current_value,
        FactAvailability::Available(72.0)
    );
    assert_ne!(performance.synchronized_value, 72.0);

    assert!(!view.nav_history.is_empty());
    assert!(view.nav_history.iter().all(|snapshot| {
        snapshot.date.as_str() >= "2025-06-01" && snapshot.date.as_str() <= "2025-06-06"
    }));
    assert_eq!(
        view.nav_history.last().map(|s| s.date.clone()).unwrap(),
        "2025-06-06"
    );
}

/// Hard database failure: no partial view, the error propagates.
#[tokio::test]
async fn view_propagates_hard_database_errors_without_partial_result() {
    let db = common::setup_test_db().await;
    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());

    // Closing the connection forces repository reads inside the composer to
    // fail, so no partial Portfolio view can be produced.
    db.close_by_ref().await.unwrap();
    let result =
        portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All).await;
    assert!(result.is_err());
}

/// The composer accepts every presentation-neutral period request and
/// returns ready history through one interface.
#[tokio::test]
async fn view_returns_requested_history_for_every_period() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVH1", "History Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2023-01-05", 1.0, 10.0, 0.0).await;
    for (date, price) in [
        ("2023-01-05", 10.0),
        ("2023-01-06", 10.0),
        ("2024-06-09", 10.0),
        ("2025-01-02", 10.0),
        ("2025-05-05", 10.0),
        ("2025-06-09", 10.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    let today = fixed_today();
    let cases = [
        (NavHistoryRequest::OneMonth, "2025-05-11"),
        (NavHistoryRequest::ThreeMonths, "2025-03-12"),
        (NavHistoryRequest::SixMonths, "2024-12-10"),
        (NavHistoryRequest::Ytd, "2025-01-01"),
        (NavHistoryRequest::OneYear, "2024-06-10"),
        (NavHistoryRequest::ThreeYears, "2023-01-04"),
        (NavHistoryRequest::FiveYears, "2023-01-04"),
        (NavHistoryRequest::All, "2023-01-04"),
    ];
    for (request, expected_start) in cases {
        // Readiness persists the full available history once; later requests
        // only read the requested window.
        let market_data = common::market_data_at(&common::MockMarketDataSources::new(), today);
        let view = portfolio_view::get_portfolio_view(&db, &market_data, request)
            .await
            .unwrap();
        let first = view
            .nav_history
            .first()
            .map(|snapshot| snapshot.date.clone())
            .unwrap_or_default();
        assert_eq!(
            first, expected_start,
            "unexpected first snapshot for request {request:?}"
        );
        assert!(view
            .nav_history
            .windows(2)
            .all(|window| window[0].date <= window[1].date));
    }
}
