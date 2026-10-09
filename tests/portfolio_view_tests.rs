#![allow(clippy::float_cmp)]

pub mod common;

use chrono::{Datelike, NaiveDate};
use rstock::models::{
    AvailablePortfolioPerformance, FactAvailability, MarketDataSubject, NavHistoryRequest,
    PeriodOutcome, PortfolioPerformance,
};
use rstock::services::metrics;
use rstock::services::portfolio_view;

fn fixed_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()
}

/// Regularly stepped observations between `start` and `end`, alternating
/// between two values so consecutive observations produce non-zero returns
/// when the values differ.
fn weekly_observations(
    start: NaiveDate,
    end: NaiveDate,
    step_days: i64,
    first: f64,
    second: f64,
) -> Vec<(String, f64)> {
    let mut values = Vec::new();
    let mut current = start;
    let mut index = 0usize;
    while current <= end {
        values.push((
            rstock::constants::format_date(current),
            if index % 2 == 0 { first } else { second },
        ));
        current = current + chrono::Duration::days(step_days);
        index += 1;
    }
    values
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

/// Reads the available performance facts of a composed view.
fn available_performance_of(
    view: &rstock::models::PortfolioView,
) -> &AvailablePortfolioPerformance {
    let PortfolioPerformance::Available(available) = &view.performance else {
        panic!("expected available performance, got {:?}", view.performance);
    };
    available
}

/// The six calculated facts of one period outcome with their names.
fn period_facts(period: &PeriodOutcome) -> [(&'static str, &FactAvailability<f64>); 6] {
    [
        ("return_pct", &period.return_pct),
        ("volatility", &period.volatility),
        ("max_drawdown", &period.max_drawdown),
        ("beta", &period.beta),
        ("sharpe", &period.sharpe),
        ("sortino", &period.sortino),
    ]
}

/// Asserts every fact of one period outcome is not applicable.
fn expect_all_facts_not_applicable(period: &PeriodOutcome, label: &str) {
    for (name, fact) in period_facts(period) {
        assert!(
            matches!(fact, FactAvailability::NotApplicable),
            "{label}.{name}: expected not applicable, got {fact:?}"
        );
    }
}

/// Asserts one fact is available with a value near `expected`.
fn expect_available_value(fact: &FactAvailability<f64>, expected: f64, label: &str) {
    let FactAvailability::Available(value) = fact else {
        panic!("{label}: expected available fact, got {fact:?}");
    };
    assert!(
        (value - expected).abs() < 1e-9,
        "{label}: {value} is not near {expected}"
    );
}

/// Sparse stock prices over several years: forward-filled NAV history stays
/// piecewise constant between observations, so period facts have exact
/// fixture values. NAV progression: 100, 110, 120, 130, 140, 150, 155 (from
/// 2024-07-01), 160 (from 2025-01-02), and 170 on 2025-06-09.
async fn seed_long_history(db: &sea_orm::DatabaseConnection) {
    let asset_id = common::insert_asset(db, "XFAKEVL1", "Long Stock", "stock", "EUR").await;
    common::insert_transaction(db, asset_id, "2019-01-02", 1.0, 10.0, 0.0).await;
    for (date, price) in [
        ("2019-01-02", 10.0),
        ("2020-01-02", 11.0),
        ("2021-01-04", 12.0),
        ("2022-01-03", 13.0),
        ("2023-01-02", 14.0),
        ("2024-01-02", 15.0),
        ("2024-07-01", 15.5),
        ("2025-01-02", 16.0),
        ("2025-06-09", 17.0),
    ] {
        common::insert_daily_price(db, asset_id, date, price, false).await;
    }
}

/// Weekly benchmark observations with alternating values plus their FX rates,
/// so the benchmark return series has non-zero returns for beta. Use equal
/// values for a constant benchmark.
fn mock_weekly_benchmark(sources: &mut common::MockMarketDataSources, first: f64, second: f64) {
    let benchmark = weekly_observations(
        NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(),
        NaiveDate::from_ymd_opt(2025, 6, 9).unwrap(),
        7,
        first,
        second,
    );
    sources
        .historical_prices
        .insert("ACWI".to_owned(), benchmark.clone());
    let rates = weekly_observations(
        NaiveDate::from_ymd_opt(2024, 12, 2).unwrap(),
        NaiveDate::from_ymd_opt(2025, 6, 9).unwrap(),
        7,
        0.9,
        0.9,
    );
    sources.exchange_rates.insert("USDEUR".to_owned(), rates);
}

/// Available performance contains YTD, 1Y, 3Y, 5Y, and All period outcomes,
/// and every period groups its return with its risk facts while facts keep
/// independent availability.
#[tokio::test]
async fn available_performance_groups_five_cohesive_period_outcomes() {
    let db = common::setup_test_db().await;
    seed_long_history(&db).await;

    let mut sources = common::MockMarketDataSources::new();
    mock_weekly_benchmark(&mut sources, 100.0, 102.0);
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();

    let performance = available_performance_of(&view);
    let periods: [&PeriodOutcome; 5] = [
        &performance.periods.ytd,
        &performance.periods.one_year,
        &performance.periods.three_years,
        &performance.periods.five_years,
        &performance.periods.all,
    ];

    for period in periods {
        for (name, fact) in period_facts(period) {
            assert!(
                matches!(fact, FactAvailability::Available(_)),
                "expected available {name}, got {fact:?}"
            );
        }
    }

    // YTD starts at the first calculable NAV point in the year (2025-01-01 at
    // NAV 155) rather than the last snapshot before the year.
    expect_available_value(
        &performance.periods.ytd.return_pct,
        (170.0 - 155.0) / 155.0 * 100.0,
        "ytd return",
    );
    // 1Y starts at the snapshot at or before the trailing year boundary
    // (2024-06-10 at NAV 150).
    expect_available_value(
        &performance.periods.one_year.return_pct,
        (170.0 - 150.0) / 150.0 * 100.0,
        "1y return",
    );
    // Drawdown of a monotonically rising NAV series is a known zero.
    expect_available_value(&performance.periods.ytd.max_drawdown, 0.0, "ytd drawdown");
}

/// All means since inception: its return is measured from the inception
/// snapshot (NAV 100.0), and it carries the same return and risk facts as
/// every other period.
#[tokio::test]
async fn all_period_means_since_inception_with_the_same_facts() {
    let db = common::setup_test_db().await;
    seed_long_history(&db).await;

    let mut sources = common::MockMarketDataSources::new();
    mock_weekly_benchmark(&mut sources, 100.0, 102.0);
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    // The inception seed snapshot has NAV 100.0, so a since-inception All
    // return must be the CAGR from that baseline, not from a later window.
    let expected = metrics::compute_cagr("2019-01-01", "2025-06-09", 100.0, 170.0).unwrap();
    assert_eq!(
        performance.periods.all.return_pct,
        FactAvailability::Available(expected)
    );

    // All carries the same fact kinds as the other periods: every fact of the
    // 1Y period that is available is available for All in the same shape.
    for (name, all_fact, one_year_fact) in [
        (
            "volatility",
            &performance.periods.all.volatility,
            &performance.periods.one_year.volatility,
        ),
        (
            "max_drawdown",
            &performance.periods.all.max_drawdown,
            &performance.periods.one_year.max_drawdown,
        ),
        (
            "beta",
            &performance.periods.all.beta,
            &performance.periods.one_year.beta,
        ),
        (
            "sharpe",
            &performance.periods.all.sharpe,
            &performance.periods.one_year.sharpe,
        ),
        (
            "sortino",
            &performance.periods.all.sortino,
            &performance.periods.one_year.sortino,
        ),
    ] {
        assert_eq!(
            all_fact.value().is_some(),
            one_year_fact.value().is_some(),
            "fact-kind mismatch for {name}"
        );
    }
}

/// YTD falls back to inception when the portfolio began during the year: the
/// first calculable NAV point of the year is the inception seed itself, so a
/// new portfolio still has a meaningful YTD result.
#[tokio::test]
async fn ytd_falls_back_to_inception_when_the_portfolio_began_during_the_year() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVN1", "New Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-03-03", 1.0, 10.0, 0.0).await;
    for (date, price) in [
        ("2025-03-03", 10.0),
        ("2025-04-01", 11.0),
        ("2025-06-09", 12.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    let sources = common::MockMarketDataSources::new();
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    // Inception seed snapshot NAV 100.0; the first in-year point is the
    // inception point itself.
    expect_available_value(&performance.periods.ytd.return_pct, 20.0, "ytd return");
    // All measures the same measurable lifetime as annualized CAGR.
    expect_available_value(
        &performance.periods.all.return_pct,
        metrics::compute_cagr("2025-03-02", "2025-06-09", 100.0, 120.0).unwrap(),
        "all return",
    );

    // No ready history reaches the trailing periods, so they are not
    // applicable rather than unavailable.
    expect_all_facts_not_applicable(&performance.periods.one_year, "one_year");
    expect_all_facts_not_applicable(&performance.periods.three_years, "three_years");
    expect_all_facts_not_applicable(&performance.periods.five_years, "five_years");
}

/// YTD falls back to inception when the ready history does not reach the
/// current year: an older Complete NAV snapshot that cannot advance still
/// reports a YTD result measured from inception, with the readiness
/// limitation coexisting with the available dated facts.
#[tokio::test]
async fn ytd_falls_back_to_inception_when_history_does_not_reach_the_year() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVST", "Stale Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2024-06-17", 1.0, 10.0, 0.0).await;
    for (date, price) in [
        ("2024-06-17", 10.0),
        ("2024-11-15", 11.0),
        ("2024-12-20", 12.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    let sources = common::MockMarketDataSources::new();
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::Ytd)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    assert_eq!(performance.effective_valuation_date, "2024-12-20");
    assert_eq!(performance.inception_date, "2024-06-16");
    // No snapshot exists in 2025, so YTD falls back to the inception seed.
    expect_available_value(&performance.periods.ytd.return_pct, 20.0, "ytd return");

    // The trailing periods do not reach back, and the readiness limitation
    // coexists with the available dated facts.
    expect_all_facts_not_applicable(&performance.periods.one_year, "one_year");
    assert!(!performance.nav_history_limitations.is_empty());
}

/// Short period history: unsupported trailing periods are fully not
/// applicable, while facts the available series can support stay available.
#[tokio::test]
async fn unsupported_periods_are_not_applicable_and_short_series_keeps_what_is_measurable() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVY1", "Young Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-04-01", 1.0, 10.0, 0.0).await;
    for (date, price) in [
        ("2025-04-01", 10.0),
        ("2025-05-01", 11.0),
        ("2025-06-09", 12.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    let sources = common::MockMarketDataSources::new();
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    expect_all_facts_not_applicable(&performance.periods.one_year, "one_year");
    expect_all_facts_not_applicable(&performance.periods.three_years, "three_years");
    expect_all_facts_not_applicable(&performance.periods.five_years, "five_years");
    expect_available_value(&performance.periods.ytd.return_pct, 20.0, "ytd return");
    expect_available_value(
        &performance.periods.all.return_pct,
        metrics::compute_cagr("2025-03-31", "2025-06-09", 100.0, 120.0).unwrap(),
        "all return",
    );
}

/// A very short series supports only the facts it has observations for:
/// volatility, drawdown, and return stay available while Sharpe, Sortino, and
/// beta are not applicable because the period history is insufficient.
#[tokio::test]
async fn short_history_makes_only_insufficient_facts_not_applicable() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVS1", "Short Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-03", 1.0, 10.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-03", 10.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 11.0, false).await;

    let sources = common::MockMarketDataSources::new();
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    let ytd = &performance.periods.ytd;
    expect_available_value(&ytd.return_pct, 10.0, "ytd return");
    assert!(matches!(ytd.volatility, FactAvailability::Available(_)));
    assert!(matches!(ytd.max_drawdown, FactAvailability::Available(_)));
    assert!(matches!(ytd.sharpe, FactAvailability::NotApplicable));
    assert!(matches!(ytd.sortino, FactAvailability::NotApplicable));
    // The series itself is too short for any 20-observation metric, so beta
    // stays not applicable even though benchmark data is also missing.
    assert!(matches!(ytd.beta, FactAvailability::NotApplicable));
}

/// A constant benchmark makes beta mathematically undefined with all
/// observations present, so beta is not applicable without any benchmark
/// limitation, while the portfolio-only facts of the same series stay
/// available.
#[tokio::test]
async fn beta_is_not_applicable_when_the_benchmark_is_constant() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVC1", "Flat Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-04-10", 1.0, 10.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-04-10", 10.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 10.0, false).await;

    let mut sources = common::MockMarketDataSources::new();
    // Constant benchmark and FX: complete observations, zero variance.
    mock_weekly_benchmark(&mut sources, 200.0, 200.0);
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    let all = &performance.periods.all;
    assert!(matches!(all.beta, FactAvailability::NotApplicable));
    // No benchmark market-data limitation exists for complete observations.
    assert!(performance.benchmark_risk_limitations.is_empty());
    // Portfolio-only facts of the same flat series stay available.
    expect_available_value(&all.volatility, 0.0, "all volatility");
    expect_available_value(&all.max_drawdown, 0.0, "all drawdown");
    expect_available_value(&all.return_pct, 0.0, "all return");
    assert!(matches!(all.sortino, FactAvailability::Available(_)));
    // Sharpe with zero excess-return dispersion is undefined too.
    assert!(matches!(all.sharpe, FactAvailability::NotApplicable));
}

/// A NAV series that never declines on a weekday inside a period makes
/// Sortino mathematically undefined, which is not applicable, while the same
/// period keeps its other facts available and the All period keeps Sortino
/// because its series includes the zero first return.
#[tokio::test]
async fn sortino_is_not_applicable_when_the_nav_series_never_declines() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVR1", "Rising Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2024-12-30", 1.0, 10.0, 0.0).await;

    // Rising every weekday: every YTD weekday return exceeds the daily
    // risk-free rate, so the 2025 YTD series has no downside returns.
    let mut current = NaiveDate::from_ymd_opt(2024, 12, 30).unwrap();
    let end = NaiveDate::from_ymd_opt(2025, 6, 9).unwrap();
    let mut price = 10.0;
    while current <= end {
        if !matches!(
            current.weekday(),
            chrono::Weekday::Sat | chrono::Weekday::Sun
        ) {
            common::insert_daily_price(
                &db,
                asset_id,
                &rstock::constants::format_date(current),
                price,
                false,
            )
            .await;
            price *= 1.01;
        }
        current += chrono::Duration::days(1);
    }

    let mut sources = common::MockMarketDataSources::new();
    // A varying benchmark so beta keeps its benchmark observations.
    let benchmark_start = NaiveDate::from_ymd_opt(2024, 12, 30).unwrap();
    sources.historical_prices.insert(
        "ACWI".to_owned(),
        weekly_observations(benchmark_start, end, 1, 100.0, 100.5),
    );
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        weekly_observations(benchmark_start, end, 1, 0.9, 0.9),
    );

    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    // The YTD series rises on every weekday, so Sortino is undefined there.
    assert!(matches!(
        performance.periods.ytd.sortino,
        FactAvailability::NotApplicable
    ));
    // The All series includes the zero return of the first day, so its
    // Sortino is defined for the same observations.
    assert!(matches!(
        performance.periods.all.sortino,
        FactAvailability::Available(_)
    ));
    // Benchmark-dependent and portfolio-only facts of the same YTD period
    // stay available.
    assert!(matches!(
        performance.periods.ytd.beta,
        FactAvailability::Available(_)
    ));
    assert!(matches!(
        performance.periods.ytd.volatility,
        FactAvailability::Available(_)
    ));
    assert!(matches!(
        performance.periods.ytd.return_pct,
        FactAvailability::Available(_)
    ));
    assert!(performance.benchmark_risk_limitations.is_empty());
}

/// Missing benchmark data makes only beta unavailable; return and the
/// portfolio-only risk facts remain computed from the NAV series, and the
/// reason stays scoped to benchmark-dependent risk rather than NAV history.
#[tokio::test]
async fn missing_benchmark_data_makes_only_beta_unavailable() {
    let db = common::setup_test_db().await;
    seed_long_history(&db).await;

    let sources = common::MockMarketDataSources::new();
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    for (name, fact) in [
        ("return_pct", &performance.periods.all.return_pct),
        ("volatility", &performance.periods.all.volatility),
        ("max_drawdown", &performance.periods.all.max_drawdown),
        ("sharpe", &performance.periods.all.sharpe),
        ("sortino", &performance.periods.all.sortino),
    ] {
        assert!(
            matches!(fact, FactAvailability::Available(_)),
            "expected available {name} without benchmark data, got {fact:?}"
        );
    }
    assert!(matches!(
        performance.periods.all.beta,
        FactAvailability::Unavailable
    ));

    // The missing benchmark is scoped to benchmark-dependent risk, not to
    // NAV/history readiness.
    assert!(!performance.benchmark_risk_limitations.is_empty());
    assert!(performance
        .benchmark_risk_limitations
        .iter()
        .any(|limitation| matches!(&limitation.subject, MarketDataSubject::Asset { ticker, .. } if ticker == "ACWI")));
    assert!(performance.nav_history_limitations.is_empty());
}

/// One view can expose all four independent limitation scopes at once:
/// current performance inventory, Monetary inventory, NAV/history readiness,
/// and benchmark-dependent risk, without any scope invalidating another.
#[tokio::test]
async fn view_reports_four_independent_limitation_scopes() {
    let db = common::setup_test_db().await;
    // Performance holding A with no market data at all: its Individual price
    // is missing, which limits the current performance inventory scope.
    let stale_id = common::insert_asset(&db, "XFAKEFSA", "Stale A", "stock", "EUR").await;
    common::insert_transaction(&db, stale_id, "2025-06-02", 1.0, 10.0, 0.0).await;
    // Performance holding B is priced after the checkpoint, so readiness
    // cannot advance NAV and the NAV/history scope stays limited.
    let blocker_id = common::insert_asset(&db, "XFAKEFSB", "Blocker B", "stock", "EUR").await;
    common::insert_transaction(&db, blocker_id, "2025-06-07", 1.0, 50.0, 0.0).await;
    // Monetary holding C with no price data limits the Monetary scope.
    let monetary_id =
        common::insert_monetary_fund_asset(&db, "XFAKEVM2", "Money Fund", "EUR", "F000VM2").await;
    common::insert_transaction(&db, monetary_id, "2025-06-06", 2.0, 1000.0, 0.0).await;

    common::insert_portfolio_snapshot(&db, "2025-06-01", 100.0, 0.0).await;
    common::insert_portfolio_snapshot(&db, "2025-06-06", 110.0, 0.2).await;
    common::insert_portfolio_asset_snapshot(&db, "2025-06-06", stale_id, 1.0, 10.0, 10.0, 1.0)
        .await;

    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert("XFAKEFSB".to_owned(), vec![("2025-06-09".to_owned(), 50.0)]);
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::OneMonth)
        .await
        .unwrap();

    let performance = available_performance_of(&view);
    assert_eq!(performance.effective_valuation_date, "2025-06-06");

    // Current performance inventory scope.
    assert!(!view
        .inventory
        .performance_holdings
        .market_data_limitations
        .is_empty());
    assert!(view
        .inventory
        .performance_holdings
        .market_data_limitations
        .iter()
        .any(|limitation| matches!(&limitation.subject, MarketDataSubject::Asset { ticker, .. } if ticker == "XFAKEFSA")));
    // Monetary inventory scope.
    assert!(!view
        .inventory
        .monetary_holdings
        .market_data_limitations
        .is_empty());
    // NAV/history readiness scope.
    assert!(!performance.nav_history_limitations.is_empty());
    // Benchmark-dependent risk scope.
    assert!(performance
        .benchmark_risk_limitations
        .iter()
        .any(|limitation| matches!(&limitation.subject, MarketDataSubject::Asset { ticker, .. } if ticker == "ACWI")));

    // A limitation in one scope does not invalidate another scope's facts:
    // the synchronized performance facts remain available at the older
    // snapshot while inventory facts stay independently dated.
    assert_eq!(performance.nav, 110.0);
    assert_eq!(view.inventory.total_value, FactAvailability::Unavailable);
}

/// Trailing periods independently reach their own boundaries: a portfolio
/// older than three but younger than five years supports the 1Y and 3Y
/// periods while 5Y is not applicable, and 5Y stays not applicable without
/// hiding 3Y.
#[tokio::test]
async fn each_trailing_period_reaches_its_own_boundary() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVMD", "Middle Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2022-01-03", 1.0, 10.0, 0.0).await;
    for (date, price) in [
        ("2022-01-03", 10.0),
        ("2023-01-02", 11.0),
        ("2024-01-02", 12.0),
        ("2025-06-09", 13.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    let sources = common::MockMarketDataSources::new();
    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    // The 3Y window reaches 2022-06-12, which the ready history covers, so
    // its return is measurable; the 5Y window reaches back to 2020-06-13,
    // before inception.
    assert!(matches!(
        performance.periods.three_years.return_pct,
        FactAvailability::Available(_)
    ));
    expect_all_facts_not_applicable(&performance.periods.five_years, "five_years");
    assert!(matches!(
        performance.periods.one_year.return_pct,
        FactAvailability::Available(_)
    ));
}

/// Beta is computed from NAV returns aligned with benchmark observations on
/// the same dates: a portfolio and benchmark stepping +1% and -1% in lockstep
/// yield beta 1.0 while the aligned pairing is exercised.
#[tokio::test]
async fn beta_uses_nav_returns_aligned_with_benchmark_observations() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEVAL", "Aligned Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-04-14", 1.0, 10.0, 0.0).await;

    // Alternating exact prices on every weekday: each weekday return is the
    // same log ratio for the portfolio (10.0/10.1) and the benchmark
    // (100.0/101.0), so the aligned covariance over variance is 1.0.
    let mut current = NaiveDate::from_ymd_opt(2025, 4, 14).unwrap();
    let end = NaiveDate::from_ymd_opt(2025, 6, 9).unwrap();
    let mut index = 0usize;
    while current <= end {
        if !matches!(
            current.weekday(),
            chrono::Weekday::Sat | chrono::Weekday::Sun
        ) {
            let stock_price = if index % 2 == 0 { 10.0 } else { 10.1 };
            common::insert_daily_price(
                &db,
                asset_id,
                &rstock::constants::format_date(current),
                stock_price,
                false,
            )
            .await;
            index += 1;
        }
        current += chrono::Duration::days(1);
    }

    let mut sources = common::MockMarketDataSources::new();
    let benchmark = weekly_observations(
        NaiveDate::from_ymd_opt(2025, 4, 14).unwrap(),
        end,
        1,
        100.0,
        101.0,
    );
    sources
        .historical_prices
        .insert("ACWI".to_owned(), benchmark);
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        weekly_observations(
            NaiveDate::from_ymd_opt(2025, 4, 14).unwrap(),
            end,
            1,
            0.9,
            0.9,
        ),
    );

    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    let FactAvailability::Available(beta) = performance.periods.all.beta else {
        panic!(
            "expected available beta, got {:?}",
            performance.periods.all.beta
        );
    };
    assert!((beta - 1.0).abs() < 1e-6, "beta {beta} is not near 1.0");
    // The alignment uses only dates with benchmark observations, which cover
    // the whole window here, so no benchmark limitation exists.
    assert!(performance.benchmark_risk_limitations.is_empty());
}

/// A benchmark that exists but only covers part of a period leaves too few
/// aligned observations for beta, making only beta unavailable while the
/// portfolio-only facts of the same period stay available.
#[tokio::test]
async fn partially_covered_benchmark_makes_only_beta_unavailable() {
    let db = common::setup_test_db().await;
    seed_long_history(&db).await;

    // Benchmark observations only for the last two weeks of the window: the
    // aligned sample stays far below the statistical minimum even though the
    // benchmark data itself is current.
    let mut sources = common::MockMarketDataSources::new();
    let window_start = NaiveDate::from_ymd_opt(2025, 5, 26).unwrap();
    sources.historical_prices.insert(
        "ACWI".to_owned(),
        weekly_observations(
            window_start,
            NaiveDate::from_ymd_opt(2025, 6, 9).unwrap(),
            7,
            100.0,
            102.0,
        ),
    );
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        weekly_observations(
            window_start,
            NaiveDate::from_ymd_opt(2025, 6, 9).unwrap(),
            7,
            0.9,
            0.9,
        ),
    );

    let market_data = common::market_data_at(&sources, fixed_today());
    let view = portfolio_view::get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .unwrap();
    let performance = available_performance_of(&view);

    assert!(matches!(
        performance.periods.all.beta,
        FactAvailability::Unavailable
    ));
    assert!(matches!(
        performance.periods.all.return_pct,
        FactAvailability::Available(_)
    ));
    assert!(matches!(
        performance.periods.all.volatility,
        FactAvailability::Available(_)
    ));
    assert!(matches!(
        performance.periods.all.sharpe,
        FactAvailability::Available(_)
    ));
    assert!(matches!(
        performance.periods.all.sortino,
        FactAvailability::Available(_)
    ));
    assert!(matches!(
        performance.periods.all.max_drawdown,
        FactAvailability::Available(_)
    ));
}
