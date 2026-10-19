//! Exposure-matched portfolio benchmark seam tests.
//!
//! The comparison is exercised offline with an in-memory database, dummy
//! assets, fixed clocks, and fake `MarketDataSources`. The fixtures are hand
//! calculated: every expected weight, return, and anchor is derived from the
//! persisted Complete NAV snapshots and the fake reference observations, so a
//! change in the comparison's arithmetic fails the test instead of silently
//! changing the reported performance.

#![allow(clippy::float_cmp)]

pub mod common;

use chrono::NaiveDate;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    Set,
};

use common::{insert_asset, market_data_at, setup_test_db, MockMarketDataSources};
use rstock::cli::adapters;
use rstock::db::entities::{
    benchmark_assignment, benchmark_reference, benchmark_reference_price, portfolio_asset_history,
    portfolio_history,
};
use rstock::models::{ComparisonAvailability, ComparisonFact, NavHistoryRequest, PortfolioView};
use rstock::services::exposure_benchmark::compute_exposure_matched_comparison;
use rstock::services::portfolio_view::get_portfolio_view;

const TODAY: NaiveDate = NaiveDate::from_ymd_opt(2025, 1, 11).expect("fixed date");
const ALPHA: &str = "ref-alpha";
const BETA: &str = "ref-beta";

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} is not within tolerance of {expected}"
    );
}

fn fixed_today() -> NaiveDate {
    TODAY
}

// --- Fixture helpers ---

async fn insert_reference(
    db: &DatabaseConnection,
    id: &str,
    name: &str,
    identifier: &str,
    currency: &str,
) {
    benchmark_reference::Entity::insert(benchmark_reference::ActiveModel {
        id: Set(id.to_owned()),
        display_name: Set(name.to_owned()),
        instrument_ticker: Set(identifier.to_owned()),
        source: Set("test_source".to_owned()),
        source_identifier: Set(identifier.to_owned()),
        currency: Set(currency.to_owned()),
        adjustment_provenance: Set("explicit fake total-return observations".to_owned()),
        adjustment_qualification: Set("dividends_reinvested_once".to_owned()),
        return_basis_qualification: Set("total_return".to_owned()),
        role: Set("Investment universe".to_owned()),
        universe: Set("test".to_owned()),
        hedge_policy: Set("Unhedged".to_owned()),
        source_provenance: Set("deterministic test source".to_owned()),
        history_boundary: Set("2025-01-06 exact fixture class".to_owned()),
        verification_state: Set("verified".to_owned()),
        ..Default::default()
    })
    .exec(db)
    .await
    .expect("reference definition should persist");
}

async fn assign_reference(
    db: &DatabaseConnection,
    subject_type: &str,
    subject: &str,
    reference: &str,
) {
    benchmark_assignment::Entity::insert(benchmark_assignment::ActiveModel {
        subject_type: Set(subject_type.to_owned()),
        subject_id: Set(subject.to_owned()),
        reference_id: Set(reference.to_owned()),
        is_override: Set(false),
        assigned_at: Set("2025-01-11T00:00:00Z".to_owned()),
    })
    .exec(db)
    .await
    .expect("assignment should persist");
}

async fn insert_nav_snapshot(
    db: &DatabaseConnection,
    date: &str,
    asset_value: f64,
    total_value: f64,
    outstanding_shares: f64,
    nav: f64,
) {
    portfolio_history::ActiveModel {
        date: Set(date.to_owned()),
        asset_value: Set(asset_value),
        total_value: Set(total_value),
        outstanding_shares: Set(outstanding_shares),
        nav: Set(nav),
    }
    .insert(db)
    .await
    .expect("NAV snapshot should persist");
}

async fn insert_exposure(
    db: &DatabaseConnection,
    date: &str,
    asset_id: i32,
    quantity: f64,
    closing_price: f64,
    market_value: f64,
) {
    portfolio_asset_history::ActiveModel {
        date: Set(date.to_owned()),
        asset_id: Set(asset_id),
        quantity: Set(quantity),
        closing_price: Set(closing_price),
        market_value: Set(market_value),
        exchange_rate: Set(1.0),
        ..Default::default()
    }
    .insert(db)
    .await
    .expect("per-asset exposure should persist");
}

/// The ready Complete NAV history of the fixture.
///
/// The seed snapshot on 2025-01-05 carries no value, the first invested close
/// is 2025-01-06, XFAKE2 is sold on 2025-01-08, XFAKE3 is bought on
/// 2025-01-09, and 60 EUR of Net dividend income is retained inside NAV from
/// 2025-01-08.
async fn insert_nav_history(db: &DatabaseConnection, alpha: i32, beta_asset: i32, gamma: i32) {
    let shares_after_sale = 6.0;
    let shares_after_buy = 6.0 + 200.0 / 120.0;
    insert_nav_snapshot(db, "2025-01-05", 0.0, 0.0, 0.0, 100.0).await;
    insert_nav_snapshot(db, "2025-01-06", 1000.0, 1000.0, 10.0, 100.0).await;
    insert_nav_snapshot(db, "2025-01-07", 1100.0, 1100.0, 10.0, 110.0).await;
    insert_nav_snapshot(db, "2025-01-08", 660.0, 720.0, shares_after_sale, 120.0).await;
    insert_nav_snapshot(db, "2025-01-09", 860.0, 920.0, shares_after_buy, 120.0).await;
    insert_nav_snapshot(
        db,
        "2025-01-10",
        940.0,
        1000.0,
        shares_after_buy,
        1000.0 / shares_after_buy,
    )
    .await;

    insert_exposure(db, "2025-01-06", alpha, 6.0, 100.0, 600.0).await;
    insert_exposure(db, "2025-01-06", beta_asset, 4.0, 100.0, 400.0).await;
    insert_exposure(db, "2025-01-07", alpha, 6.0, 110.0, 660.0).await;
    insert_exposure(db, "2025-01-07", beta_asset, 4.0, 110.0, 440.0).await;
    insert_exposure(db, "2025-01-08", alpha, 6.0, 110.0, 660.0).await;
    insert_exposure(db, "2025-01-09", alpha, 6.0, 110.0, 660.0).await;
    insert_exposure(db, "2025-01-09", gamma, 2.0, 100.0, 200.0).await;
    insert_exposure(db, "2025-01-10", alpha, 6.0, 120.0, 720.0).await;
    insert_exposure(db, "2025-01-10", gamma, 2.0, 110.0, 220.0).await;
}

/// The fake reference observations.
///
/// ALPHA moves +10% on 2025-01-07 and +10% on 2025-01-09. BETA moves +10% on
/// 2025-01-08. Both report every completed weekday of the window they
/// contribute to.
fn reference_sources() -> MockMarketDataSources {
    let mut sources = MockMarketDataSources::new();
    sources.historical_prices.insert(
        "ALPHA".to_owned(),
        vec![
            ("2025-01-03".to_owned(), 100.0),
            ("2025-01-06".to_owned(), 100.0),
            ("2025-01-07".to_owned(), 110.0),
            ("2025-01-08".to_owned(), 110.0),
            ("2025-01-09".to_owned(), 121.0),
            ("2025-01-10".to_owned(), 121.0),
        ],
    );
    sources.historical_prices.insert(
        "BETA".to_owned(),
        vec![
            ("2025-01-03".to_owned(), 50.0),
            ("2025-01-06".to_owned(), 50.0),
            ("2025-01-07".to_owned(), 50.0),
            ("2025-01-08".to_owned(), 55.0),
        ],
    );
    for identifier in ["ALPHA", "BETA"] {
        sources.series_qualifications.insert(
            ("test_source".to_owned(), identifier.to_owned()),
            qualification("EUR"),
        );
    }
    sources
}

/// The qualified source registration of a fake reference.
fn qualification(currency: &str) -> rstock::services::market_data::SeriesQualification {
    rstock::services::market_data::SeriesQualification {
        currency: currency.to_owned(),
        return_basis: "total_return".to_owned(),
        adjustment: "dividends_reinvested_once".to_owned(),
        hedge_policy: "Unhedged".to_owned(),
        cadence: "daily".to_owned(),
    }
}

/// The complete exposure fixture: assets, references, assignments, and the
/// persisted Complete NAV history.
async fn exposure_fixture() -> (DatabaseConnection, MockMarketDataSources) {
    let db = setup_test_db().await;
    let alpha = insert_asset(&db, "XFAKE1", "Alpha Stock", "stock", "EUR").await;
    let beta_asset = insert_asset(&db, "XFAKE2", "Beta Stock", "stock", "EUR").await;
    let gamma = insert_asset(&db, "XFAKE3", "Gamma Stock", "stock", "EUR").await;
    insert_reference(&db, ALPHA, "Verified Alpha Reference", "ALPHA", "EUR").await;
    insert_reference(&db, BETA, "Verified Beta Reference", "BETA", "EUR").await;
    assign_reference(&db, "tracked", "XFAKE1", ALPHA).await;
    assign_reference(&db, "tracked", "XFAKE2", BETA).await;
    assign_reference(&db, "tracked", "XFAKE3", ALPHA).await;
    insert_nav_history(&db, alpha, beta_asset, gamma).await;
    (db, reference_sources())
}

/// The ready Complete NAV snapshots of the fixture, in chronology.
fn ready_snapshots() -> Vec<rstock::models::PortfolioSnapshot> {
    let shares_after_buy = 6.0 + 200.0 / 120.0;
    vec![
        snapshot("2025-01-05", 0.0, 0.0, 0.0, 100.0),
        snapshot("2025-01-06", 1000.0, 1000.0, 10.0, 100.0),
        snapshot("2025-01-07", 1100.0, 1100.0, 10.0, 110.0),
        snapshot("2025-01-08", 660.0, 720.0, 6.0, 120.0),
        snapshot("2025-01-09", 860.0, 920.0, shares_after_buy, 120.0),
        snapshot(
            "2025-01-10",
            940.0,
            1000.0,
            shares_after_buy,
            1000.0 / shares_after_buy,
        ),
    ]
}

fn snapshot(
    date: &str,
    asset_value: f64,
    total_value: f64,
    outstanding_shares: f64,
    nav: f64,
) -> rstock::models::PortfolioSnapshot {
    rstock::models::PortfolioSnapshot {
        date: date.to_owned(),
        asset_value,
        total_value,
        outstanding_shares,
        nav,
    }
}

async fn compute_fixture_comparison(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
) -> rstock::models::ExposureBenchmarkComparison {
    let market_data = market_data_at(sources, fixed_today());
    compute_exposure_matched_comparison(db, &market_data, &ready_snapshots())
        .await
        .expect("the exposure comparison should be computable")
}

/// The JSON envelope of one Portfolio view.
fn view_json(view: &PortfolioView) -> serde_json::Value {
    let mut output = Vec::new();
    adapters::write_portfolio_json(&mut output, view).expect("JSON should be written");
    common::parse_json_envelope(&output)
}

/// The human output of one Portfolio view.
fn view_human(view: &PortfolioView) -> String {
    let mut output = Vec::new();
    adapters::write_portfolio_human(&mut output, view, NavHistoryRequest::All)
        .expect("human output should be written");
    String::from_utf8(output).expect("human output should be UTF-8")
}

/// A weekend fixture whose portfolio is reallocated between the Friday and
/// the Monday close.
///
/// The ready history covers 2025-01-05 (the seed) through 2025-01-13 (the
/// Monday close). On Friday the portfolio is 100% asset A. Over the weekend it
/// is reallocated to 50% A and 50% B, and on Monday A is flat while B gains
/// 10%. The retained Net dividend income is optional: it is included when
/// `dividend` is positive.
async fn weekend_fixture(dividend: f64) -> (DatabaseConnection, MockMarketDataSources) {
    let db = setup_test_db().await;
    let alpha_asset = insert_asset(&db, "XFAKE1", "Alpha Stock", "stock", "EUR").await;
    let beta_asset = insert_asset(&db, "XFAKE2", "Beta Stock", "stock", "EUR").await;
    insert_reference(&db, ALPHA, "Verified Alpha Reference", "ALPHA", "EUR").await;
    insert_reference(&db, BETA, "Verified Beta Reference", "BETA", "EUR").await;
    assign_reference(&db, "tracked", "XFAKE1", ALPHA).await;
    assign_reference(&db, "tracked", "XFAKE2", BETA).await;

    insert_nav_snapshot(&db, "2025-01-05", 0.0, 0.0, 0.0, 100.0).await;
    for date in [
        "2025-01-06",
        "2025-01-07",
        "2025-01-08",
        "2025-01-09",
        "2025-01-10",
        "2025-01-11",
    ] {
        insert_nav_snapshot(&db, date, 1000.0, 1000.0, 10.0, 100.0).await;
        insert_exposure(&db, date, alpha_asset, 10.0, 100.0, 1000.0).await;
    }
    // The Sunday close holds the reallocated scope.
    insert_nav_snapshot(&db, "2025-01-12", 1000.0, 1000.0 + dividend, 10.0, 100.0).await;
    insert_exposure(&db, "2025-01-12", alpha_asset, 5.0, 100.0, 500.0).await;
    insert_exposure(&db, "2025-01-12", beta_asset, 5.0, 100.0, 500.0).await;
    // The Monday close keeps the reallocation and gains on B only.
    insert_nav_snapshot(
        &db,
        "2025-01-13",
        1050.0,
        1050.0 + dividend,
        10.0,
        105.0 + dividend / 10.0,
    )
    .await;
    insert_exposure(&db, "2025-01-13", alpha_asset, 5.0, 100.0, 500.0).await;
    insert_exposure(&db, "2025-01-13", beta_asset, 5.0, 110.0, 550.0).await;

    let mut sources = MockMarketDataSources::new();
    sources.historical_prices.insert(
        "ALPHA".to_owned(),
        vec![
            ("2025-01-03".to_owned(), 100.0),
            ("2025-01-06".to_owned(), 100.0),
            ("2025-01-07".to_owned(), 100.0),
            ("2025-01-08".to_owned(), 100.0),
            ("2025-01-09".to_owned(), 100.0),
            ("2025-01-10".to_owned(), 100.0),
            ("2025-01-13".to_owned(), 100.0),
        ],
    );
    sources.historical_prices.insert(
        "BETA".to_owned(),
        vec![
            ("2025-01-03".to_owned(), 50.0),
            ("2025-01-06".to_owned(), 50.0),
            ("2025-01-07".to_owned(), 50.0),
            ("2025-01-08".to_owned(), 50.0),
            ("2025-01-09".to_owned(), 50.0),
            ("2025-01-10".to_owned(), 50.0),
            ("2025-01-13".to_owned(), 55.0),
        ],
    );
    for identifier in ["ALPHA", "BETA"] {
        sources.series_qualifications.insert(
            ("test_source".to_owned(), identifier.to_owned()),
            qualification("EUR"),
        );
    }
    (db, sources)
}

/// The ready snapshots of the weekend fixture.
fn weekend_snapshots(dividend: f64) -> Vec<rstock::models::PortfolioSnapshot> {
    let mut snapshots = vec![snapshot("2025-01-05", 0.0, 0.0, 0.0, 100.0)];
    for date in [
        "2025-01-06",
        "2025-01-07",
        "2025-01-08",
        "2025-01-09",
        "2025-01-10",
        "2025-01-11",
    ] {
        snapshots.push(snapshot(date, 1000.0, 1000.0, 10.0, 100.0));
    }
    snapshots.push(snapshot(
        "2025-01-12",
        1000.0,
        1000.0 + dividend,
        10.0,
        100.0,
    ));
    snapshots.push(snapshot(
        "2025-01-13",
        1050.0,
        1050.0 + dividend,
        10.0,
        105.0 + dividend / 10.0,
    ));
    snapshots
}

// --- Tests ---

/// The comparison weights every interval with the *preceding* Complete NAV
/// snapshot and combines the assigned primary references through them.
#[tokio::test]
async fn exposure_comparison_uses_prior_close_weights_and_anchors_both_series() {
    let (db, sources) = exposure_fixture().await;
    let comparison = compute_fixture_comparison(&db, &sources).await;

    assert_eq!(comparison.availability, ComparisonAvailability::Available);
    assert_eq!(comparison.start_date.as_deref(), Some("2025-01-06"));
    assert_eq!(comparison.end_date.as_deref(), Some("2025-01-10"));
    assert_eq!(comparison.inception_date.as_deref(), Some("2025-01-05"));
    assert!(comparison.retrospective);

    // Both series are anchored at 100 on the first invested closing snapshot.
    assert_eq!(
        comparison.subject_wealth.first().unwrap().date,
        "2025-01-06"
    );
    assert_close(comparison.subject_wealth.first().unwrap().wealth, 100.0);
    assert_eq!(
        comparison.reference_wealth.first().unwrap().date,
        "2025-01-06"
    );
    assert_close(comparison.reference_wealth.first().unwrap().wealth, 100.0);

    // The subject is the portfolio NAV rebased at the anchor.
    let subject: Vec<f64> = comparison
        .subject_wealth
        .iter()
        .map(|point| point.wealth)
        .collect();
    assert_close(subject[1], 110.0);
    assert_close(subject[2], 120.0);
    assert_close(subject[3], 120.0);
    assert_close(subject[4], 1000.0 / (6.0 + 200.0 / 120.0));

    // The reference compounds the weighted prior-close reference returns:
    // 0.60 * +10% on 01-07, 0.40 * +10% on 01-08, 0.916667 * +10% on 01-09.
    let reference: Vec<f64> = comparison
        .reference_wealth
        .iter()
        .map(|point| point.wealth)
        .collect();
    assert_close(reference[1], 106.0);
    assert_close(reference[2], 110.24);
    assert_close(reference[3], 110.24 * (1.0 + 660.0 / 720.0 * 0.1));
    assert_close(reference[4], reference[3]);

    assert_close(
        comparison.subject_return_pct.unwrap(),
        1000.0 / (6.0 + 200.0 / 120.0) - 100.0,
    );
    assert_close(
        comparison.reference_return_pct.unwrap(),
        20.345_333_333_333_332,
    );
    assert_close(
        comparison.excess_return_percentage_points.unwrap(),
        comparison.subject_return_pct.unwrap() - comparison.reference_return_pct.unwrap(),
    );
}

/// The seed-to-first-close interval is disclosed instead of being compared.
#[tokio::test]
async fn comparison_discloses_the_omitted_seed_to_first_close_interval() {
    let (db, sources) = exposure_fixture().await;
    let comparison = compute_fixture_comparison(&db, &sources).await;

    let omitted = comparison
        .omitted_seed_interval
        .expect("the seed interval must be disclosed");
    assert_eq!(omitted.seed_date, "2025-01-05");
    assert_eq!(omitted.anchor_date, "2025-01-06");
}

/// A trade on a closing date changes the *next* interval's weights only.
#[tokio::test]
async fn same_day_trade_changes_only_the_following_interval_weights() {
    let (db, sources) = exposure_fixture().await;
    let comparison = compute_fixture_comparison(&db, &sources).await;

    // The closed asset's reference still carries 40% on its sale close and
    // reports no weight afterwards, so the sale changes only the following
    // interval's contribution.
    let beta = comparison
        .references
        .iter()
        .find(|reference| reference.reference_id == BETA)
        .expect("the closed asset's reference must stay visible");
    assert_close(beta.first_weight_pct.unwrap(), 40.0);
    assert_close(beta.last_weight_pct.unwrap(), 0.0);
    assert_close(beta.max_weight_pct.unwrap(), 40.0);
    assert_eq!(beta.assets, vec!["XFAKE2".to_owned()]);

    // The 2025-01-08 interval still carries the sold asset's 40% weight: the
    // reference wealth of that interval is 106.0 * 1.04, not 106.0 * 1.10.
    let reference: Vec<f64> = comparison
        .reference_wealth
        .iter()
        .map(|point| point.wealth)
        .collect();
    assert_close(reference[1], 106.0);
    assert_close(reference[2], 110.24);
}

/// The retained Net dividend income is carried inside the NAV scope at a 0%
/// return instead of being reinvested or dropped.
#[tokio::test]
async fn retained_dividend_income_is_carried_at_zero_return() {
    let (db, sources) = exposure_fixture().await;
    let comparison = compute_fixture_comparison(&db, &sources).await;

    // On 2025-01-08 the held asset is 660 EUR of a 720 EUR NAV scope, so the
    // residual 60 EUR of Net dividend income carries 0% return.
    let reference: Vec<f64> = comparison
        .reference_wealth
        .iter()
        .map(|point| point.wealth)
        .collect();
    assert_close(reference[3], 110.24 * (1.0 + 660.0 / 720.0 * 0.1));
}

/// Assets that share one primary reference aggregate their weights.
#[tokio::test]
async fn assets_sharing_one_reference_aggregate_their_weights() {
    let (db, sources) = exposure_fixture().await;
    let comparison = compute_fixture_comparison(&db, &sources).await;

    let alpha = comparison
        .references
        .iter()
        .find(|reference| reference.reference_id == ALPHA)
        .expect("the shared reference must be reported once");
    assert_eq!(alpha.assets, vec!["XFAKE1".to_owned(), "XFAKE3".to_owned()]);
    // 660 + 200 EUR of 920 EUR of NAV scope on the last invested close.
    assert_close(alpha.last_weight_pct.unwrap(), 860.0 / 920.0 * 100.0);
    assert_close(alpha.max_weight_pct.unwrap(), 860.0 / 920.0 * 100.0);
    assert_close(alpha.first_weight_pct.unwrap(), 60.0);
    assert_eq!(
        comparison
            .references
            .iter()
            .filter(|reference| reference.reference_id == ALPHA)
            .count(),
        1,
        "a shared reference must not be counted twice"
    );
}

/// A missing explicit assignment makes the comparison unavailable; the
/// remaining weights are never renormalized, and the selected references stay
/// visible.
#[tokio::test]
async fn missing_assignment_is_unavailable_and_never_renormalized() {
    let (db, sources) = exposure_fixture().await;
    benchmark_assignment::Entity::delete_by_id(("tracked".to_owned(), "XFAKE2".to_owned()))
        .exec(&db)
        .await
        .expect("the assignment should be removable");

    let comparison = compute_fixture_comparison(&db, &sources).await;
    match &comparison.availability {
        ComparisonAvailability::Unavailable { reason } => {
            assert!(
                reason.contains("XFAKE2"),
                "the unassigned asset must be named: {reason}"
            );
            assert!(
                reason.contains("not renormalized"),
                "the reason must state that weights are not renormalized: {reason}"
            );
        }
        other => panic!("expected an unavailable comparison, got {other:?}"),
    }
    assert!(comparison.subject_wealth.is_empty());
    // The selected reference provenance stays visible on the unavailable
    // outcome.
    let alpha = comparison
        .references
        .iter()
        .find(|reference| reference.reference_id == ALPHA)
        .expect("the selected reference must stay visible");
    assert_eq!(alpha.display_name, "Verified Alpha Reference");
    assert_eq!(alpha.return_basis, "total_return");
    assert_eq!(alpha.adjustment, "dividends_reinvested_once");
    assert_eq!(alpha.hedge_policy, "Unhedged");
    assert_eq!(alpha.native_currency, "EUR");
    assert_eq!(alpha.source, "test_source");
    assert_eq!(alpha.source_identifier, "ALPHA");
    assert_eq!(alpha.verification_state, "verified");
    assert_eq!(alpha.assigned_at.as_deref(), Some("2025-01-11T00:00:00Z"));
    assert!(alpha.first_weight_pct.is_none());
}

/// Secondary and candidate references never enter the exposure weights.
#[tokio::test]
async fn secondary_and_candidate_assignments_never_enter_the_weights() {
    let (db, sources) = exposure_fixture().await;
    // A candidate comparator and a non-primary subject type must not become
    // portfolio exposure.
    assign_reference(&db, "fund_candidate", "fund:LU0000000000", BETA).await;
    assign_reference(&db, "tracked_secondary", "XFAKE1", BETA).await;

    let comparison = compute_fixture_comparison(&db, &sources).await;
    let alpha = comparison
        .references
        .iter()
        .find(|reference| reference.reference_id == ALPHA)
        .expect("the primary reference must stay in place");
    assert_eq!(alpha.assets, vec!["XFAKE1".to_owned(), "XFAKE3".to_owned()]);
    assert_close(alpha.first_weight_pct.unwrap(), 60.0);
    let beta = comparison
        .references
        .iter()
        .find(|reference| reference.reference_id == BETA)
        .expect("the closed asset's reference must stay visible");
    assert_eq!(beta.assets, vec!["XFAKE2".to_owned()]);
    assert_close(beta.first_weight_pct.unwrap(), 40.0);
}

/// A reference without a qualified observation on an internal weekday makes
/// the comparison unavailable: a forward-filled mark is not an independent
/// observation.
#[tokio::test]
async fn a_missing_internal_weekday_observation_makes_the_comparison_unavailable() {
    let (db, mut sources) = exposure_fixture().await;
    // ALPHA stops reporting on 2025-01-09. The source has no observation for
    // that weekday, so the comparison cannot measure the interval.
    sources.historical_prices.insert(
        "ALPHA".to_owned(),
        vec![
            ("2025-01-03".to_owned(), 100.0),
            ("2025-01-06".to_owned(), 100.0),
            ("2025-01-07".to_owned(), 110.0),
            ("2025-01-08".to_owned(), 110.0),
            ("2025-01-10".to_owned(), 121.0),
        ],
    );

    let comparison = compute_fixture_comparison(&db, &sources).await;
    match &comparison.availability {
        ComparisonAvailability::Unavailable { reason } => {
            assert!(
                reason.contains("no qualified observation"),
                "the missing observation must be named: {reason}"
            );
            assert!(
                reason.contains("2025-01-09"),
                "the missing close must be named: {reason}"
            );
        }
        other => panic!("expected an unavailable comparison, got {other:?}"),
    }
    // The reference provenance stays visible on the unavailable outcome.
    assert!(comparison
        .references
        .iter()
        .any(|reference| reference.reference_id == ALPHA));
}

/// The comparison reads the exposures in one bounded range and prepares each
/// reference once: no per-date query and no per-date source request.
#[tokio::test]
async fn comparison_uses_bounded_reads_and_one_request_per_reference() {
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());
    compute_exposure_matched_comparison(&db, &market_data, &ready_snapshots())
        .await
        .expect("the exposure comparison should be computable");

    let requests = sources.historical_price_request_ranges();
    let reference_requests: Vec<_> = requests
        .iter()
        .filter(|(identifier, _, _)| identifier == "ALPHA" || identifier == "BETA")
        .collect();
    assert_eq!(
        reference_requests.len(),
        2,
        "one request per contributing reference, not per date: {requests:?}"
    );
    let alpha_request = reference_requests
        .iter()
        .find(|(identifier, _, _)| *identifier == "ALPHA")
        .expect("ALPHA must be requested once");
    assert_eq!(
        alpha_request.1,
        NaiveDate::from_ymd_opt(2025, 1, 6).unwrap()
    );
    assert_eq!(
        alpha_request.2,
        NaiveDate::from_ymd_opt(2025, 1, 10).unwrap()
    );
    // The closed asset's reference is only prepared for the window it
    // contributes to.
    let beta_request = reference_requests
        .iter()
        .find(|(identifier, _, _)| *identifier == "BETA")
        .expect("BETA must be requested once");
    assert_eq!(beta_request.1, NaiveDate::from_ymd_opt(2025, 1, 6).unwrap());
    assert_eq!(beta_request.2, NaiveDate::from_ymd_opt(2025, 1, 8).unwrap());
}

/// The comparison never rebuilds NAV: the persisted history is untouched and
/// NAV readiness still reports zero execution reads afterwards.
#[tokio::test]
async fn the_comparison_performs_no_second_nav_rebuild() {
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());
    let before = portfolio_history::Entity::find()
        .all(&db)
        .await
        .expect("NAV history should be readable")
        .len();

    compute_exposure_matched_comparison(&db, &market_data, &ready_snapshots())
        .await
        .expect("the exposure comparison should be computable");

    let after = portfolio_history::Entity::find()
        .all(&db)
        .await
        .expect("NAV history should be readable")
        .len();
    assert_eq!(after, before, "the comparison must not write NAV history");
    let readiness = rstock::services::nav::ensure_portfolio_history(&db, &market_data)
        .await
        .expect("NAV readiness should succeed on the untouched history");
    assert_eq!(
        readiness.execution_database_reads, 0,
        "the untouched history needs no rebuild"
    );
}

/// A benchmark failure leaves the inventory, the NAV readiness, and the
/// portfolio-only outcomes intact.
#[tokio::test]
async fn benchmark_failure_leaves_the_portfolio_view_intact() {
    let (db, mut sources) = exposure_fixture().await;
    sources.failing_price_sources.insert("ALPHA".to_owned());
    let market_data = market_data_at(&sources, fixed_today());

    let view = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("a benchmark failure must not fail the Portfolio view");
    match &view.exposure_benchmark.availability {
        ComparisonAvailability::Unavailable { reason } => {
            assert!(
                reason.contains("coverage is incomplete"),
                "the failure reason must be visible: {reason}"
            );
        }
        other => panic!("expected an unavailable comparison, got {other:?}"),
    }
    // The portfolio-only outcomes stay available.
    assert!(matches!(
        view.performance,
        rstock::models::PortfolioPerformance::Available(_)
    ));
    assert!(!view.nav_history.is_empty());
    // The reference provenance and its limitations stay visible.
    let alpha = view
        .exposure_benchmark
        .references
        .iter()
        .find(|reference| reference.reference_id == ALPHA)
        .expect("the selected reference must stay visible");
    assert_eq!(alpha.display_name, "Verified Alpha Reference");
    assert!(!alpha.limitations.is_empty());
    assert!(!view.exposure_benchmark.reference_limitations.is_empty());
}

/// A missing FX rate for a non-EUR reference keeps the inventory, the ready
/// NAV, and the portfolio-only outcomes intact.
#[tokio::test]
async fn a_missing_fx_rate_keeps_the_portfolio_only_outcomes() {
    let db = setup_test_db().await;
    let alpha = insert_asset(&db, "XFAKE1", "Alpha Stock", "stock", "EUR").await;
    let beta_asset = insert_asset(&db, "XFAKE2", "Beta Stock", "stock", "EUR").await;
    let gamma = insert_asset(&db, "XFAKE3", "Gamma Stock", "stock", "EUR").await;
    insert_reference(&db, ALPHA, "Verified Alpha Reference", "ALPHA", "USD").await;
    assign_reference(&db, "tracked", "XFAKE1", ALPHA).await;
    assign_reference(&db, "tracked", "XFAKE2", ALPHA).await;
    assign_reference(&db, "tracked", "XFAKE3", ALPHA).await;
    insert_nav_history(&db, alpha, beta_asset, gamma).await;

    let mut sources = reference_sources();
    sources.series_qualifications.insert(
        ("test_source".to_owned(), "ALPHA".to_owned()),
        qualification("USD"),
    );
    let market_data = market_data_at(&sources, fixed_today());

    let view = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("a missing FX rate must not fail the Portfolio view");
    assert!(matches!(
        view.exposure_benchmark.availability,
        ComparisonAvailability::Unavailable { .. }
    ));
    assert!(matches!(
        view.performance,
        rstock::models::PortfolioPerformance::Available(_)
    ));
    assert!(!view.nav_history.is_empty());
    // The FX absence is classified with its requested end date.
    assert!(view
        .exposure_benchmark
        .reference_limitations
        .iter()
        .any(|limitation| matches!(
            limitation.subject,
            rstock::models::MarketDataSubject::FxRate { .. }
        )));
}

/// A failed database read is a hard error, not an unavailable comparison.
#[tokio::test]
async fn a_failed_database_read_is_a_hard_error() {
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());
    db.execute_unprepared("DROP TABLE portfolio_asset_history")
        .await
        .expect("the exposure table should be droppable");

    let error = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect_err("a failed database read must fail the Portfolio view");
    assert!(
        error.to_string().contains("portfolio_asset_history")
            || error.to_string().contains("no such table"),
        "the database failure must be visible: {error}"
    );
}

/// A malformed persisted reference price is a hard error, not an unavailable
/// comparison.
#[tokio::test]
async fn a_malformed_persisted_reference_price_is_a_hard_error() {
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());
    benchmark_reference_price::ActiveModel {
        reference_id: Set(ALPHA.to_owned()),
        date: Set("2025-01-05".to_owned()),
        closing_price: Set(0.0),
        is_api_failure: Set(false),
        is_source_observation: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .expect("the malformed price should persist");

    let error = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect_err("a malformed persisted price must fail the Portfolio view");
    assert!(
        error
            .to_string()
            .contains("invalid persisted benchmark reference price"),
        "the malformed observation must be visible: {error}"
    );
}

/// A mismatched exposure total is a hard error, not an unavailable
/// comparison.
#[tokio::test]
async fn a_mismatched_exposure_total_is_a_hard_error() {
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());
    let row = portfolio_asset_history::Entity::find()
        .filter(portfolio_asset_history::Column::Date.eq("2025-01-07"))
        .one(&db)
        .await
        .expect("the exposure row should be readable")
        .expect("the exposure row should exist");
    let mut active: portfolio_asset_history::ActiveModel = row.into();
    active.market_value = Set(600.0);
    active
        .update(&db)
        .await
        .expect("the exposure row should update");

    let error = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect_err("a mismatched exposure total must fail the Portfolio view");
    assert!(
        error
            .to_string()
            .contains("do not match the persisted asset value"),
        "the invariant violation must be visible: {error}"
    );
}

/// An empty portfolio keeps its domain outcomes and carries no comparison.
#[tokio::test]
async fn empty_portfolio_retains_its_domain_outcomes() {
    let db = setup_test_db().await;
    let sources = MockMarketDataSources::new();
    let market_data = market_data_at(&sources, fixed_today());
    let view = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("an empty portfolio should compose");
    assert_eq!(
        view.exposure_benchmark.availability,
        ComparisonAvailability::NotApplicable {
            reason: "portfolio has no invested Complete NAV snapshot to anchor an exposure-matched comparison".to_owned()
        }
    );
    assert_eq!(
        view.performance,
        rstock::models::PortfolioPerformance::NotApplicable
    );
    assert!(view.inventory.performance_holdings.positions.is_empty());
}

/// A Monetary-only portfolio keeps its domain outcomes and carries no
/// comparison.
#[tokio::test]
async fn monetary_only_portfolio_retains_its_domain_outcomes() {
    let db = setup_test_db().await;
    let monetary =
        common::insert_monetary_fund_asset(&db, "XFAKEM1", "Money Fund", "EUR", "MONEYCODE").await;
    common::insert_transaction(&db, monetary, "2025-01-06", 1.0, 100.0, 0.0).await;
    let mut sources = MockMarketDataSources::new();
    sources.historical_prices.insert(
        "MONEYCODE".to_owned(),
        vec![("2025-01-06".to_owned(), 100.0)],
    );
    let market_data = market_data_at(&sources, fixed_today());

    let view = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("a Monetary-only portfolio should compose");
    assert!(matches!(
        view.exposure_benchmark.availability,
        ComparisonAvailability::NotApplicable { .. }
    ));
    assert!(view.inventory.monetary_holdings.positions.len() == 1);
    assert!(view.inventory.performance_holdings.positions.is_empty());
}

/// The human and JSON Adapters report the same exposure facts.
#[tokio::test]
async fn human_and_json_adapters_report_the_same_exposure_facts() {
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());
    let view: PortfolioView = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("the Portfolio view should compose");

    let json = view_json(&view);
    let comparison = &json["data"]["exposure_benchmark"];
    assert_eq!(comparison["availability"]["status"], "available");
    assert_eq!(comparison["start_date"], "2025-01-06");
    assert_eq!(comparison["end_date"], "2025-01-10");
    assert_eq!(
        comparison["omitted_seed_interval"]["seed_date"],
        "2025-01-05"
    );
    assert_eq!(comparison["subject_wealth"][0]["wealth"], 100.0);
    assert_eq!(comparison["reference_wealth"][0]["wealth"], 100.0);
    assert_eq!(comparison["references"][0]["reference_id"], ALPHA);
    assert_eq!(
        comparison["references"][0]["assigned_at"],
        "2025-01-11T00:00:00Z"
    );
    assert_eq!(
        comparison["standard_periods"][4]["period"],
        "since_comparison_start"
    );
    assert_eq!(
        comparison["standard_periods"][4]["start_date"],
        "2025-01-06"
    );

    let human = view_human(&view);
    assert!(human.contains("Exposure-matched benchmark:"));
    assert!(human.contains("Primary exposure comparison: available"));
    assert!(human.contains("06-01-2025"));
    assert!(human.contains("Verified Alpha Reference"));
    assert!(human.contains("Since Comparison Start"));
    assert!(human.contains("selected retrospectively"));
    // The assignment moment is shown in the display date format while the
    // JSON keeps its machine value.
    assert!(human.contains("assigned 11-01-2025"));
    assert!(!human.contains("2025-01-11T00:00:00Z"));
}

/// The human and JSON Adapters report the same facts for an unavailable
/// comparison, including the reference provenance and its limitations.
#[tokio::test]
async fn human_and_json_adapters_report_the_same_unavailable_facts() {
    let (db, mut sources) = exposure_fixture().await;
    sources.failing_price_sources.insert("ALPHA".to_owned());
    let market_data = market_data_at(&sources, fixed_today());
    let view = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("a benchmark failure must not fail the Portfolio view");

    let json = view_json(&view);
    let comparison = &json["data"]["exposure_benchmark"];
    assert_eq!(comparison["availability"]["status"], "unavailable");
    assert!(comparison["availability"]["reason"]
        .as_str()
        .unwrap()
        .contains("coverage is incomplete"));
    assert_eq!(comparison["references"][0]["reference_id"], ALPHA);
    assert_eq!(
        comparison["references"][0]["verification_state"],
        "verified"
    );
    assert!(!comparison["reference_limitations"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(!comparison["references"][0]["limitations"]
        .as_array()
        .unwrap()
        .is_empty());

    let human = view_human(&view);
    assert!(human.contains("Primary exposure comparison: unavailable"));
    assert!(human.contains("Verified Alpha Reference"));
    assert!(human.contains("reference limitation:"));
}

/// The standard rows report the comparison's own dates and never depend on a
/// graph period.
#[tokio::test]
async fn standard_rows_report_the_comparison_anchors() {
    let (db, sources) = exposure_fixture().await;
    let comparison = compute_fixture_comparison(&db, &sources).await;

    assert_eq!(comparison.standard_periods.len(), 5);
    let since = comparison
        .standard_periods
        .iter()
        .find(|period| {
            period.period == rstock::models::BenchmarkComparisonPeriod::SinceComparisonStart
        })
        .expect("the since-comparison-start row must be present");
    assert_eq!(since.start_date.as_deref(), Some("2025-01-06"));
    assert_eq!(since.end_date.as_deref(), Some("2025-01-10"));
    assert!(matches!(
        since.subject_return_pct,
        ComparisonFact::Available { value } if (value - 30.434_782_608_695_66).abs() < 1e-9
    ));
    assert!(matches!(
        since.reference_return_pct,
        ComparisonFact::Available { value } if (value - 20.345_333_333_333_332).abs() < 1e-9
    ));
    assert!(matches!(
        since.excess_return_percentage_points,
        ComparisonFact::Available { value } if (value - 10.089_449_275_362_329).abs() < 1e-9
    ));
    // Five observations are below the risk-sample minimum, so the risk facts
    // are not applicable rather than unavailable.
    assert!(matches!(since.beta, ComparisonFact::NotApplicable { .. }));
}

/// A trade over a non-trading day is part of the scope that measured the
/// Monday interval: the immediately preceding Complete close carries the
/// reallocated weights, not the stale Friday close.
#[tokio::test]
async fn a_weekend_trade_is_reflected_in_the_monday_interval_weights() {
    let (db, sources) = weekend_fixture(0.0).await;
    let market_data = market_data_at(&sources, NaiveDate::from_ymd_opt(2025, 1, 14).unwrap());
    let comparison =
        compute_exposure_matched_comparison(&db, &market_data, &weekend_snapshots(0.0))
            .await
            .expect("the exposure comparison should be computable");

    assert_eq!(comparison.availability, ComparisonAvailability::Available);
    assert_eq!(comparison.start_date.as_deref(), Some("2025-01-06"));
    assert_eq!(comparison.end_date.as_deref(), Some("2025-01-13"));

    // The Monday interval weighs the Sunday scope: 50% of a flat A and 50% of
    // a B that gained 10%, so the exposure-matched benchmark gains 5% and not
    // the 0% of the stale Friday weights.
    let reference: Vec<f64> = comparison
        .reference_wealth
        .iter()
        .map(|point| point.wealth)
        .collect();
    assert_close(reference[reference.len() - 1], 105.0);
    assert_close(comparison.reference_return_pct.unwrap(), 5.0);
    // The portfolio itself gained the same 5% over that interval.
    assert_close(comparison.subject_return_pct.unwrap(), 5.0);
}

/// The retained Net dividend income of a weekend dividend is carried inside
/// the complete NAV scope at a 0% return.
#[tokio::test]
async fn a_weekend_dividend_cash_is_carried_at_zero_return() {
    let (db, sources) = weekend_fixture(100.0).await;
    let market_data = market_data_at(&sources, NaiveDate::from_ymd_opt(2025, 1, 14).unwrap());
    let comparison =
        compute_exposure_matched_comparison(&db, &market_data, &weekend_snapshots(100.0))
            .await
            .expect("the exposure comparison should be computable");

    // The Sunday scope is 500 EUR of A, 500 EUR of B, and 100 EUR of retained
    // Net dividend income over a 1,100 EUR NAV scope. The cash carries a 0%
    // return, so the interval gains 500/1100 * 10% = 4.5455%.
    let reference: Vec<f64> = comparison
        .reference_wealth
        .iter()
        .map(|point| point.wealth)
        .collect();
    assert_close(
        reference[reference.len() - 1],
        100.0 * (1.0 + 500.0 / 1100.0 * 0.1),
    );
}

/// A persisted close without any per-asset exposure scope is an invariant
/// violation, not an expected absence.
#[tokio::test]
async fn a_deleted_exposure_scope_is_a_hard_invariant_error() {
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());
    portfolio_asset_history::Entity::delete_many()
        .filter(portfolio_asset_history::Column::Date.eq("2025-01-07"))
        .exec(&db)
        .await
        .expect("the exposure rows should be deletable");

    let error = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect_err("a deleted exposure scope must fail the Portfolio view");
    assert!(
        error
            .to_string()
            .contains("no per-asset exposure scope for an asset value of"),
        "the invariant violation must be visible: {error}"
    );
}

/// A not-applicable comparison reports not-applicable standard facts, never
/// the contradictory unavailable state.
#[tokio::test]
async fn a_not_applicable_comparison_reports_not_applicable_standard_facts() {
    let db = setup_test_db().await;
    let sources = MockMarketDataSources::new();
    let market_data = market_data_at(&sources, fixed_today());
    let view = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("an empty portfolio should compose");
    assert!(matches!(
        view.exposure_benchmark.availability,
        ComparisonAvailability::NotApplicable { .. }
    ));
    assert!(view
        .exposure_benchmark
        .standard_periods
        .iter()
        .all(|period| matches!(
            period.subject_return_pct,
            ComparisonFact::NotApplicable { .. }
        )));
    assert!(view
        .exposure_benchmark
        .standard_periods
        .iter()
        .all(|period| matches!(
            period.reference_return_pct,
            ComparisonFact::NotApplicable { .. }
        )));

    let json = view_json(&view);
    let comparison = &json["data"]["exposure_benchmark"];
    assert_eq!(comparison["availability"]["status"], "not_applicable");
    for period in comparison["standard_periods"].as_array().unwrap() {
        assert_eq!(period["subject_return_pct"]["status"], "not_applicable");
        assert_eq!(period["reference_return_pct"]["status"], "not_applicable");
        assert_eq!(period["beta"]["status"], "not_applicable");
    }
    let human = view_human(&view);
    assert!(human.contains("Primary exposure comparison: not applicable"));
}

/// A missing qualified observation is reported with a scoped, classified
/// limitation that carries the date gap.
#[tokio::test]
async fn a_missing_qualified_observation_reports_a_scoped_limitation() {
    let (db, mut sources) = exposure_fixture().await;
    // ALPHA stops reporting on 2025-01-09, an internal completed weekday.
    sources.historical_prices.insert(
        "ALPHA".to_owned(),
        vec![
            ("2025-01-03".to_owned(), 100.0),
            ("2025-01-06".to_owned(), 100.0),
            ("2025-01-07".to_owned(), 110.0),
            ("2025-01-08".to_owned(), 110.0),
            ("2025-01-10".to_owned(), 121.0),
        ],
    );

    let comparison = compute_fixture_comparison(&db, &sources).await;
    assert!(matches!(
        comparison.availability,
        ComparisonAvailability::Unavailable { .. }
    ));
    let limitation = comparison
        .reference_limitations
        .iter()
        .find(|limitation| {
            matches!(
                limitation.subject,
                rstock::models::MarketDataSubject::BenchmarkReference { .. }
            )
        })
        .expect("the missing observation must carry a scoped limitation");
    assert_eq!(
        limitation.requested_end_date,
        NaiveDate::from_ymd_opt(2025, 1, 9).unwrap()
    );
    assert_eq!(
        limitation.latest_available_date,
        Some(NaiveDate::from_ymd_opt(2025, 1, 8).unwrap())
    );
    assert_eq!(
        limitation.classification,
        rstock::models::MarketDataLimitationClassification::ActionableMissingData
    );
    match &limitation.subject {
        rstock::models::MarketDataSubject::BenchmarkReference { id, display_name } => {
            assert_eq!(id, ALPHA);
            assert_eq!(display_name, "Verified Alpha Reference");
        }
        other => panic!("the limitation must name the reference, got {other:?}"),
    }

    // The same limitation is visible in the human and the JSON output.
    let market_data = market_data_at(&sources, fixed_today());
    let view = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("a missing observation must not fail the Portfolio view");
    let json = view_json(&view);
    let json_limitations = json["data"]["exposure_benchmark"]["reference_limitations"]
        .as_array()
        .expect("the limitations must be an array");
    assert!(!json_limitations.is_empty());
    let json_limitation = json_limitations
        .iter()
        .find(|limitation| limitation["subject"]["type"] == "benchmark_reference")
        .expect("the JSON limitation must name the reference");
    assert_eq!(json_limitation["requested_end_date"], "2025-01-09");
    assert_eq!(json_limitation["latest_available_date"], "2025-01-08");
    assert_eq!(json_limitation["classification"], "actionable_missing_data");

    // The human Adapter shows the same limitation in the display date format.
    let human = view_human(&view);
    assert!(human.contains("reference limitation:"));
    assert!(human.contains("09-01-2025"));
    assert!(human.contains("08-01-2025"));
}

/// The fund comparison keeps its accepted `fund_return_pct` row key while the
/// portfolio comparison exposes the shared subject key, and both carry the
/// same computed facts.
#[tokio::test]
async fn fund_and_portfolio_rows_keep_their_own_keys_with_matching_facts() {
    let (db, sources) = fund_fixture().await;
    let market_data = market_data_at(&sources, NaiveDate::from_ymd_opt(2025, 1, 8).unwrap());
    let analysis = rstock::services::fund_analysis::compute_fund_analysis(
        &db,
        &market_data,
        "XCLASS",
        rstock::models::CandidateCorrelationPeriod {
            label: "1Y",
            days: 365,
        },
    )
    .await
    .expect("the fund analysis should compose");
    let comparison = &analysis.primary_benchmark_comparison;
    assert_eq!(
        comparison.availability,
        ComparisonAvailability::Available,
        "the fund comparison must stay available"
    );

    let mut output = Vec::new();
    rstock::cli::output::write_json(&mut output, "analyze.fund", &analysis)
        .expect("the fund analysis JSON should serialize");
    let fund_json = common::parse_json_envelope(&output);
    let fund_rows = &fund_json["data"]["primary_benchmark_comparison"]["standard_periods"];
    let fund_row = &fund_rows[4];
    assert_eq!(fund_row["fund_return_pct"]["status"], "available");
    assert!(
        fund_row.get("subject_return_pct").is_none(),
        "the accepted fund contract must not change: {fund_row}"
    );

    // The shared calculation over the same series produces the same fact
    // under the neutral subject name.
    let subject: Vec<(String, f64)> = comparison
        .fund_wealth
        .iter()
        .map(|point| (point.date.clone(), point.wealth))
        .collect();
    let reference: Vec<(String, f64)> = comparison
        .reference_wealth
        .iter()
        .map(|point| (point.date.clone(), point.wealth))
        .collect();
    let shared = rstock::services::benchmark::compute_standard_benchmark_periods(
        &subject,
        &reference,
        comparison.start_date.as_deref().unwrap(),
        comparison.end_date.as_deref().unwrap(),
    )
    .expect("the shared calculation should succeed");
    assert_eq!(
        shared[4].subject_return_pct, comparison.standard_periods[4].fund_return_pct,
        "the shared row and the fund row must carry the same computed fact"
    );

    // The portfolio comparison exposes the explicit subject key.
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());
    let view = get_portfolio_view(&db, &market_data, NavHistoryRequest::All)
        .await
        .expect("the Portfolio view should compose");
    let portfolio_json = view_json(&view);
    let portfolio_row = &portfolio_json["data"]["exposure_benchmark"]["standard_periods"][4];
    assert_eq!(portfolio_row["subject_return_pct"]["status"], "available");
    assert!(
        portfolio_row.get("fund_return_pct").is_none(),
        "the portfolio comparison must expose its own subject key: {portfolio_row}"
    );
    assert_eq!(
        portfolio_row["subject_return_pct"]["value"],
        portfolio_json["data"]["exposure_benchmark"]["subject_return_pct"]
    );
}

/// Both dashboard aliases report the same exposure comparison through the
/// production command dispatch.
#[tokio::test]
async fn both_dashboard_aliases_report_the_same_exposure_comparison() {
    let (db, sources) = exposure_fixture().await;
    let market_data = market_data_at(&sources, fixed_today());

    let aliases: [fn() -> rstock::cli::Commands; 2] = [
        || rstock::cli::Commands::Get {
            period: rstock::cli::ChartPeriod::All,
        },
        || {
            rstock::cli::Commands::Portfolio(rstock::cli::PortfolioArgs {
                command: rstock::cli::PortfolioCommands::Get {
                    period: rstock::cli::ChartPeriod::All,
                },
            })
        },
    ];
    for alias in aliases {
        let (json, human) = dispatch_dashboard(&db, &market_data, alias).await;
        assert_eq!(json["command"], "portfolio.get");
        assert_eq!(
            json["data"]["exposure_benchmark"]["availability"]["status"],
            "available"
        );
        assert_eq!(
            json["data"]["exposure_benchmark"]["start_date"],
            "2025-01-06"
        );
        assert_eq!(json["data"]["exposure_benchmark"]["end_date"], "2025-01-10");
        assert_eq!(
            json["data"]["exposure_benchmark"]["references"][0]["reference_id"],
            ALPHA
        );
        assert!(human.contains("Primary exposure comparison: available"));
        assert!(human.contains("Exposure-matched benchmark:"));
    }
}

/// Dispatches one dashboard alias through the production command dispatch and
/// captures both output formats.
async fn dispatch_dashboard(
    db: &DatabaseConnection,
    market_data: &rstock::services::market_data::MarketData,
    make_command: fn() -> rstock::cli::Commands,
) -> (serde_json::Value, String) {
    let mut json_output = Vec::new();
    rstock::cli::run_command_with_writer(
        db,
        market_data,
        make_command(),
        rstock::cli::output::OutputFormat::Json,
        &mut json_output,
    )
    .await
    .expect("the dashboard alias should dispatch");
    let json = common::parse_json_envelope(&json_output);

    let mut human_output = Vec::new();
    rstock::cli::run_command_with_writer(
        db,
        market_data,
        make_command(),
        rstock::cli::output::OutputFormat::Human,
        &mut human_output,
    )
    .await
    .expect("the dashboard alias should dispatch");
    let human = String::from_utf8(human_output).expect("human output should be UTF-8");
    (json, human)
}

/// A fund analysis fixture whose assigned primary reference is the same
/// verified reference the exposure fixture uses.
async fn fund_fixture() -> (DatabaseConnection, MockMarketDataSources) {
    let db = setup_test_db().await;
    common::insert_fund_asset(&db, "XISINFAKE1", "Fund", "EUR", "XCLASS").await;
    insert_reference(&db, ALPHA, "Verified Alpha Reference", "ALPHA", "EUR").await;
    assign_reference(&db, "tracked", "XISINFAKE1", ALPHA).await;

    let mut sources = MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XCLASS".to_owned(),
        vec![
            ("2025-01-06".to_owned(), 100.0),
            ("2025-01-07".to_owned(), 112.0),
        ],
    );
    sources.historical_prices.insert(
        "ALPHA".to_owned(),
        vec![
            ("2025-01-03".to_owned(), 100.0),
            ("2025-01-06".to_owned(), 100.0),
            ("2025-01-07".to_owned(), 110.0),
            ("2025-01-08".to_owned(), 110.0),
            ("2025-01-09".to_owned(), 121.0),
            ("2025-01-10".to_owned(), 121.0),
        ],
    );
    sources.series_qualifications.insert(
        ("morningstar".to_owned(), "XCLASS".to_owned()),
        qualification("EUR"),
    );
    sources.series_qualifications.insert(
        ("test_source".to_owned(), "ALPHA".to_owned()),
        qualification("EUR"),
    );
    sources.fund_data.insert(
        "XCLASS".to_owned(),
        rstock::models::FundData {
            fund_currency: Some("EUR".to_owned()),
            total_holdings: None,
            portfolio_date: None,
            holdings: vec![],
        },
    );
    sources.fund_quote_metadata.insert(
        "XCLASS".to_owned(),
        rstock::models::FundQuoteMetadata {
            quote_currency: Some("EUR".to_owned()),
            ..Default::default()
        },
    );
    (db, sources)
}
