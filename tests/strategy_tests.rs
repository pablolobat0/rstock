//! Explicit Strategy benchmark selection seam tests.
//!
//! The selection, the unselection, the replacement, and the in-use deletion
//! guard are exercised offline with an in-memory database, dummy identities,
//! a fixed clock, and fake `MarketDataSources`. The selection commands run
//! through the library-owned production dispatch, so the tests observe exactly
//! the behaviour of `rstock strategy ...`.

#![allow(dead_code)]

mod common;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use clap::Parser;
use migration::{Migrator, MigratorTrait};
use rstock::{
    cli::{output::OutputFormat, Cli, Commands},
    db::{
        entities::{
            benchmark_blend_component, benchmark_reference, benchmark_reference_price,
            benchmark_strategy_selection, daily_exchange_rate, portfolio_history,
        },
        repos::portfolio_history_repo,
    },
    models::{
        BenchmarkComparisonPeriod, ComparisonAvailability, ComparisonFact, PortfolioSnapshot,
    },
    services::{
        custom_blends::{self, BlendCreateRequest},
        market_data::SeriesQualification,
        strategy,
    },
};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection,
    EntityTrait, QueryFilter, QueryOrder, Set, Statement,
};

use common::{market_data_at, setup_test_db, MockMarketDataSources};

const REFERENCE_A: &str = "x-strategy-reference-a";
const REFERENCE_B: &str = "x-strategy-reference-b";
const SOURCE: &str = "test_source";
const WINDOW_START: &str = "2025-01-02";
const WINDOW_END: &str = "2025-02-04";
const STRATEGY_ROLE: &str = "strategy";

/// The clock the fixtures use.
fn clock() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date")
}

/// The number of `down` steps that reaches the Strategy selection migration.
///
/// The migration is located by its stable name, so an unrelated additive
/// migration cannot shift the boundary this test exercises.
fn migrations_from_strategy_selection() -> u32 {
    let migrations = Migrator::migrations();
    let index = migrations
        .iter()
        .position(|migration| migration.name().contains("add_strategy_selection"))
        .expect("the strategy selection migration should be registered");
    (migrations.len() - index) as u32
}

/// A component definition whose qualification is verified and daily.
async fn insert_verified_reference(db: &DatabaseConnection, id: &str, identifier: &str) {
    insert_verified_reference_in(db, id, identifier, "EUR").await;
}

/// A component definition in the given currency, whose qualification is
/// verified and daily.
async fn insert_verified_reference_in(
    db: &DatabaseConnection,
    id: &str,
    identifier: &str,
    currency: &str,
) {
    benchmark_reference::ActiveModel {
        id: Set(id.to_owned()),
        display_name: Set(format!("Strategy reference {identifier}")),
        instrument_ticker: Set(identifier.to_owned()),
        source: Set(SOURCE.to_owned()),
        source_identifier: Set(identifier.to_owned()),
        currency: Set(currency.to_owned()),
        adjustment_provenance: Set("Adjusted daily closes".to_owned()),
        adjustment_qualification: Set("dividends_reinvested_once".to_owned()),
        return_basis_qualification: Set("total_return".to_owned()),
        role: Set("Investment universe".to_owned()),
        universe: Set("Equities".to_owned()),
        hedge_policy: Set("Unhedged".to_owned()),
        source_provenance: Set("Fake offline source".to_owned()),
        history_boundary: Set("Fake offline history".to_owned()),
        verification_state: Set("verified".to_owned()),
        official_comparator: Set(None),
        objective_comparator: Set(None),
        kind: Set("reference".to_owned()),
    }
    .insert(db)
    .await
    .expect("failed to insert the reference fixture");
}

/// The verified daily qualification of the fake components.
fn qualification() -> SeriesQualification {
    qualification_in("EUR")
}

/// The verified daily qualification of a fake component in the given
/// currency.
fn qualification_in(currency: &str) -> SeriesQualification {
    SeriesQualification {
        currency: currency.to_owned(),
        return_basis: "total_return".to_owned(),
        adjustment: "dividends_reinvested_once".to_owned(),
        hedge_policy: "Unhedged".to_owned(),
        cadence: "daily".to_owned(),
    }
}

/// The weekday observations of one fake component between two dates.
fn weekday_observations(
    start: &str,
    end: &str,
    mut price: impl FnMut(&str) -> f64,
) -> Vec<(String, f64)> {
    let mut day = NaiveDate::parse_from_str(start, "%Y-%m-%d").expect("valid start date");
    let last = NaiveDate::parse_from_str(end, "%Y-%m-%d").expect("valid end date");
    let mut observations = Vec::new();
    while day <= last {
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            let date = day.format("%Y-%m-%d").to_string();
            let value = price(&date);
            observations.push((date, value));
        }
        day += Duration::days(1);
    }
    observations
}

/// A component price series that steps on the given dates.
fn stepped_observations(start: &str, end: &str, steps: &[(&str, f64)]) -> Vec<(String, f64)> {
    weekday_observations(start, end, |date| {
        steps
            .iter()
            .filter(|(step_date, _)| *step_date <= date)
            .map(|(_, price)| *price)
            .last()
            .unwrap_or(100.0)
    })
}

/// The offline fixture: two verified components.
fn offline_sources(
    observations_a: Vec<(String, f64)>,
    observations_b: Vec<(String, f64)>,
) -> MockMarketDataSources {
    let mut sources = MockMarketDataSources::new();
    sources
        .historical_prices
        .insert("XREFA".to_owned(), observations_a);
    sources
        .historical_prices
        .insert("XREFB".to_owned(), observations_b);
    sources
        .series_qualifications
        .insert((SOURCE.to_owned(), "XREFA".to_owned()), qualification());
    sources
        .series_qualifications
        .insert((SOURCE.to_owned(), "XREFB".to_owned()), qualification());
    sources
}

/// The offline fixture without one completed weekday of a component.
///
/// The cached close of that weekday disappears with the observation, so the
/// component really has an interior weekday gap.
fn sources_without(
    sources: &MockMarketDataSources,
    identifier: &str,
    excluded: &str,
) -> MockMarketDataSources {
    let mut gapped = sources.clone();
    if let Some(observations) = gapped.historical_prices.get_mut(identifier) {
        observations.retain(|(date, _)| date != excluded);
    }
    gapped
}

/// The offline fixture with a stepped component and a flat second component.
fn stepped_sources() -> MockMarketDataSources {
    offline_sources(
        stepped_observations(
            WINDOW_START,
            WINDOW_END,
            &[("2025-01-15", 100.0), ("2025-01-16", 110.0)],
        ),
        weekday_observations(WINDOW_START, WINDOW_END, |_| 100.0),
    )
}

/// The offline fixture of a component in another currency.
///
/// The exchange rates convert the component into the base currency, so the
/// persisted conversion is exercised.
fn foreign_currency_sources() -> MockMarketDataSources {
    let mut sources = MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XREFA".to_owned(),
        weekday_observations(WINDOW_START, WINDOW_END, |_| 100.0),
    );
    sources.series_qualifications.insert(
        (SOURCE.to_owned(), "XREFA".to_owned()),
        qualification_in("USD"),
    );
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        weekday_observations(WINDOW_START, WINDOW_END, |_| 0.5),
    );
    sources
}

/// The portfolio fixture: one invested Complete closing snapshot per weekday,
/// with a NAV that rises by one point per weekday.
///
/// The window holds 24 weekdays, so the NAV series is 100.0 on 02-01-2025 and
/// 123.0 on 04-02-2025, and the portfolio's cumulative return over the window
/// is exactly 23%.
async fn insert_invested_portfolio(db: &DatabaseConnection) {
    let asset_id = common::insert_asset(&db, "XFAKE1", "Invested fixture", "stock", "EUR").await;
    let mut nav = 100.0;
    for (date, _) in weekday_observations(WINDOW_START, WINDOW_END, |_| 0.0) {
        common::insert_portfolio_snapshot(db, &date, nav, 10.0).await;
        common::insert_portfolio_asset_snapshot(db, &date, asset_id, 10.0, nav, nav * 10.0, 1.0)
            .await;
        nav += 1.0;
    }
}

/// The ready Complete NAV snapshots of the fixture.
async fn ready_snapshots(db: &DatabaseConnection) -> Vec<PortfolioSnapshot> {
    portfolio_history_repo::find_between(db, WINDOW_START, WINDOW_END)
        .await
        .expect("the ready NAV history should be readable")
}

/// Saves one Custom blend whose component steps on 16-01-2025.
///
/// The component holds 60% and synthetic cash holds 40%, so the blend's wealth
/// rises by exactly 6% on 16-01-2025 and stays flat afterwards.
async fn save_stepped_blend(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
    today: NaiveDate,
) -> String {
    let market_data = market_data_at(sources, today);
    let definition = custom_blends::create_custom_blend(
        db,
        &market_data,
        BlendCreateRequest {
            name: "Sixty Forty".to_owned(),
            components: vec![
                rstock::models::BlendComponentArgument {
                    identity: REFERENCE_A.to_owned(),
                    weight_percent: 60.0,
                },
                rstock::models::BlendComponentArgument {
                    identity: "cash".to_owned(),
                    weight_percent: 40.0,
                },
            ],
            start_date: Some(
                NaiveDate::parse_from_str(WINDOW_START, "%Y-%m-%d").expect("valid start date"),
            ),
        },
    )
    .await
    .expect("the blend should be saved");
    definition.id
}

/// The value of one available comparison fact.
fn available_value(fact: &ComparisonFact<f64>) -> f64 {
    match fact {
        ComparisonFact::Available { value } => *value,
        other => panic!("expected an available fact, got {other:?}"),
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
}

/// The wealth of one dated point of a comparison series.
fn wealth_at(series: &[rstock::models::DatedWealthPoint], date: &str) -> f64 {
    series
        .iter()
        .find(|point| point.date == date)
        .map(|point| point.wealth)
        .unwrap_or_else(|| panic!("the series has no point on {date}"))
}

/// Asserts that a human output holds no stored ISO date.
fn assert_no_iso_dates(human: &str) {
    let bytes = human.as_bytes();
    for index in 0..bytes.len().saturating_sub(9) {
        let window = &bytes[index..index + 10];
        let digit = |byte: &u8| byte.is_ascii_digit();
        if window[4] == b'-'
            && window[7] == b'-'
            && window[0..4].iter().all(digit)
            && window[5..7].iter().all(digit)
            && window[8..10].iter().all(digit)
        {
            let date = std::str::from_utf8(window).expect("the window is ascii");
            panic!("the human output shows the stored ISO date {date}: {human}");
        }
    }
}

/// Parses one Strategy command from the command line.
fn strategy_command(args: &[&str]) -> Commands {
    match Cli::try_parse_from(args)
        .expect("the command should parse")
        .command
    {
        Commands::Strategy(arguments) => Commands::Strategy(arguments),
        other => panic!("expected a strategy command, got {other:?}"),
    }
}

/// Runs one Strategy command through the library-owned production dispatch.
async fn run_strategy(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
    today: NaiveDate,
    args: &[&str],
    format: OutputFormat,
) -> anyhow::Result<Vec<u8>> {
    let market_data = market_data_at(sources, today);
    let mut output = Vec::new();
    rstock::cli::run_command_with_writer(
        db,
        &market_data,
        strategy_command(args),
        format,
        &mut output,
    )
    .await?;
    Ok(output)
}

#[tokio::test]
async fn selecting_a_strategy_persists_the_explicit_selection() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;

    let market_data = market_data_at(&sources, today);
    let outcome = strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");
    assert!(outcome.selected);
    assert_eq!(outcome.reference_id.as_deref(), Some(blend_id.as_str()));
    assert_eq!(outcome.display_name.as_deref(), Some("Sixty Forty"));
    assert_eq!(
        outcome.selection_basis.as_deref(),
        Some(strategy::SELECTION_BASIS_EXPLICIT)
    );
    assert_eq!(outcome.selected_at.as_deref(), Some("2025-02-05T00:00:00Z"));
    assert!(outcome.replaced_reference_id.is_none());

    let persisted = strategy::selected_strategy(&db)
        .await
        .expect("the selection should be readable");
    let persisted = persisted.expect("a strategy should be selected");
    assert_eq!(persisted.reference_id, blend_id);
    assert_eq!(
        persisted.selection_basis,
        strategy::SELECTION_BASIS_EXPLICIT
    );
    assert_eq!(persisted.selected_at, "2025-02-05T00:00:00Z");
}

#[tokio::test]
async fn the_selection_is_never_inferred_from_the_portfolio_holdings() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    save_stepped_blend(&db, &sources, today).await;
    insert_invested_portfolio(&db).await;

    assert!(
        strategy::selected_strategy(&db)
            .await
            .expect("the selection should be readable")
            .is_none(),
        "a strategy must never be inferred from the holdings"
    );

    let market_data = market_data_at(&sources, today);
    let comparison =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect("the comparison should be reported");
    assert!(matches!(
        comparison.availability,
        ComparisonAvailability::NotApplicable { .. }
    ));
    assert_eq!(comparison.role, STRATEGY_ROLE);
    assert_eq!(comparison.method, "explicit_strategy_selection");
    assert_eq!(comparison.reference_label, "Strategy benchmark");
    assert!(comparison.standard_periods.len() == 5);
}

#[tokio::test]
async fn unselecting_removes_the_persisted_selection_and_is_repeatable() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let outcome = strategy::unselect_strategy(&db)
        .await
        .expect("the strategy should be unselected");
    assert!(!outcome.selected);
    assert_eq!(
        outcome.replaced_reference_id.as_deref(),
        Some(blend_id.as_str())
    );
    assert_eq!(
        outcome.replaced_display_name.as_deref(),
        Some("Sixty Forty")
    );
    assert!(strategy::selected_strategy(&db)
        .await
        .expect("the selection should be readable")
        .is_none());

    let repeated = strategy::unselect_strategy(&db)
        .await
        .expect("unselecting again should be a no-op");
    assert!(!repeated.selected);
    assert!(repeated.replaced_reference_id.is_none());
}

#[tokio::test]
async fn selecting_another_strategy_replaces_the_current_selection() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = stepped_sources();
    let today = clock();
    let first = save_stepped_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &first)
        .await
        .expect("the first strategy should be selected");

    let second = custom_blends::create_custom_blend(
        &db,
        &market_data,
        BlendCreateRequest {
            name: "All Equity".to_owned(),
            components: vec![rstock::models::BlendComponentArgument {
                identity: REFERENCE_B.to_owned(),
                weight_percent: 100.0,
            }],
            start_date: Some(
                NaiveDate::parse_from_str(WINDOW_START, "%Y-%m-%d").expect("valid start date"),
            ),
        },
    )
    .await
    .expect("the second blend should be saved");

    let outcome = strategy::select_strategy(&db, &market_data, &second.id)
        .await
        .expect("the second strategy should be selected");
    assert_eq!(outcome.reference_id.as_deref(), Some(second.id.as_str()));
    assert_eq!(
        outcome.replaced_reference_id.as_deref(),
        Some(first.as_str())
    );
    assert_eq!(
        outcome.replaced_display_name.as_deref(),
        Some("Sixty Forty")
    );

    let persisted = strategy::selected_strategy(&db)
        .await
        .expect("the selection should be readable")
        .expect("a strategy should be selected");
    assert_eq!(persisted.reference_id, second.id);
}

#[tokio::test]
async fn a_selected_strategy_cannot_be_deleted_until_it_is_unselected() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let failure = custom_blends::delete_custom_blend(&db, &blend_id)
        .await
        .expect_err("a selected strategy must not be deletable");
    assert!(
        failure.to_string().contains("Strategy benchmark"),
        "the failure must name the Strategy benchmark: {failure}"
    );
    assert!(
        custom_blends::show_custom_blend(&db, &blend_id)
            .await
            .is_ok(),
        "the rejected deletion must leave the definition saved"
    );
    let cached_prices = benchmark_reference_price::Entity::find()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(REFERENCE_A))
        .all(&db)
        .await
        .expect("the cached prices should be readable");
    assert!(
        !cached_prices.is_empty(),
        "the rejected deletion must preserve the cached market data"
    );

    strategy::unselect_strategy(&db)
        .await
        .expect("the strategy should be unselected");
    let deletion = custom_blends::delete_custom_blend(&db, &blend_id)
        .await
        .expect("an unused definition should be deletable");
    assert!(deletion.deleted);
    assert!(
        custom_blends::show_custom_blend(&db, &blend_id)
            .await
            .is_err(),
        "the deleted definition must be gone"
    );
    let cached_prices = benchmark_reference_price::Entity::find()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(REFERENCE_A))
        .all(&db)
        .await
        .expect("the cached prices should be readable");
    assert!(
        !cached_prices.is_empty(),
        "the deletion must preserve the cached market data"
    );
}

#[tokio::test]
async fn the_strategy_comparison_is_labelled_separately_and_uses_monthly_reset_arithmetic() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let comparison =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect("the comparison should be calculated");
    assert_eq!(comparison.availability, ComparisonAvailability::Available);
    assert_eq!(comparison.role, STRATEGY_ROLE);
    assert_eq!(comparison.method, "explicit_strategy_selection");
    assert_eq!(comparison.subject_label, "Portfolio");
    assert_eq!(comparison.reference_label, "Sixty Forty");
    assert_eq!(comparison.definition_kind.as_deref(), Some("custom_blend"));
    assert_eq!(comparison.currency, "EUR");
    assert_eq!(comparison.start_date.as_deref(), Some(WINDOW_START));
    assert_eq!(comparison.end_date.as_deref(), Some(WINDOW_END));
    assert_eq!(
        comparison.strategy_anchor_date.as_deref(),
        Some(WINDOW_START)
    );
    assert_eq!(comparison.reset_rule.as_deref(), Some("monthly"));
    assert_eq!(comparison.components.len(), 2);
    assert_eq!(
        comparison.selection_basis.as_deref(),
        Some(strategy::SELECTION_BASIS_EXPLICIT)
    );

    assert_close(
        comparison.subject_return_pct.expect("a portfolio return"),
        23.0,
    );
    assert_close(
        comparison.reference_return_pct.expect("a strategy return"),
        6.0,
    );
    assert_close(
        comparison
            .excess_return_percentage_points
            .expect("an excess return"),
        17.0,
    );

    // The blend compounds an arithmetic 6% interval return on 16-01-2025 and
    // stays flat afterwards; the monthly reset does not change the wealth.
    assert_close(wealth_at(&comparison.reference_wealth, WINDOW_START), 100.0);
    assert_close(wealth_at(&comparison.reference_wealth, "2025-01-15"), 100.0);
    assert_close(wealth_at(&comparison.reference_wealth, "2025-01-16"), 106.0);
    assert_close(wealth_at(&comparison.reference_wealth, WINDOW_END), 106.0);
    assert_close(wealth_at(&comparison.subject_wealth, WINDOW_START), 100.0);
    assert_close(wealth_at(&comparison.subject_wealth, WINDOW_END), 123.0);

    let since_start = comparison
        .standard_periods
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::SinceComparisonStart)
        .expect("a since-comparison-start row");
    assert_eq!(since_start.start_date.as_deref(), Some(WINDOW_START));
    assert_eq!(since_start.end_date.as_deref(), Some(WINDOW_END));
    assert_close(available_value(&since_start.subject_return_pct), 23.0);
    assert_close(available_value(&since_start.reference_return_pct), 6.0);
    assert_close(
        available_value(&since_start.excess_return_percentage_points),
        17.0,
    );
}

#[tokio::test]
async fn the_standard_rows_never_shorten_a_window_the_strategy_cannot_cover() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let comparison =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect("the comparison should be calculated");
    for period in [
        BenchmarkComparisonPeriod::OneYear,
        BenchmarkComparisonPeriod::ThreeYear,
        BenchmarkComparisonPeriod::FiveYear,
    ] {
        let row = comparison
            .standard_periods
            .iter()
            .find(|row| row.period == period)
            .expect("a standard row");
        assert!(
            matches!(row.subject_return_pct, ComparisonFact::NotApplicable { .. }),
            "the {period:?} row must not be computed on a shortened window"
        );
    }
    let ytd = comparison
        .standard_periods
        .iter()
        .find(|row| row.period == BenchmarkComparisonPeriod::Ytd)
        .expect("a year-to-date row");
    assert_eq!(ytd.start_date.as_deref(), Some(WINDOW_START));
}

#[tokio::test]
async fn a_later_start_strategy_exposes_its_anchor_and_the_unavailable_earlier_coverage() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let market_data = market_data_at(&sources, today);
    let later_start =
        NaiveDate::parse_from_str("2025-01-16", "%Y-%m-%d").expect("valid start date");
    let definition = custom_blends::create_custom_blend(
        &db,
        &market_data,
        BlendCreateRequest {
            name: "Later Start".to_owned(),
            components: vec![rstock::models::BlendComponentArgument {
                identity: REFERENCE_A.to_owned(),
                weight_percent: 100.0,
            }],
            start_date: Some(later_start),
        },
    )
    .await
    .expect("the blend should be saved");
    insert_invested_portfolio(&db).await;
    strategy::select_strategy(&db, &market_data, &definition.id)
        .await
        .expect("the strategy should be selected");

    let comparison =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect("the comparison should be calculated");
    assert_eq!(comparison.availability, ComparisonAvailability::Available);
    assert_eq!(
        comparison.strategy_anchor_date.as_deref(),
        Some("2025-01-16"),
        "the strategy must expose its own anchor"
    );
    assert_eq!(comparison.start_date.as_deref(), Some("2025-01-16"));
    let limitation = comparison
        .earlier_period_limitation
        .as_ref()
        .expect("the earlier coverage must be disclosed");
    assert!(
        limitation.contains("2025-01-16"),
        "the limitation must name the anchor: {limitation}"
    );
    let since_start = comparison
        .standard_periods
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::SinceComparisonStart)
        .expect("a since-comparison-start row");
    assert_eq!(since_start.start_date.as_deref(), Some("2025-01-16"));
}

#[tokio::test]
async fn an_unavailable_strategy_leaves_the_portfolio_facts_intact() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let history_before = common::get_all_snapshots(&db).await;
    let assets_before = benchmark_blend_component::Entity::find()
        .order_by_asc(benchmark_blend_component::Column::BlendId)
        .all(&db)
        .await
        .expect("the saved definition should be readable");

    // The component's cached history disappears and its source fails, so the
    // selected Strategy benchmark cannot be prepared.
    benchmark_reference_price::Entity::delete_many()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(REFERENCE_A))
        .exec(&db)
        .await
        .expect("the cached prices should be removable");
    let mut failing_sources = sources.clone();
    failing_sources
        .failing_price_sources
        .insert("XREFA".to_owned());
    let failing_market_data = market_data_at(&failing_sources, today);

    let comparison = strategy::compute_strategy_comparison(
        &db,
        &failing_market_data,
        &ready_snapshots(&db).await,
    )
    .await
    .expect("an unavailable strategy must not fail the command");
    assert!(
        matches!(
            comparison.availability,
            ComparisonAvailability::Unavailable { .. }
        ),
        "the strategy must be unavailable, got {:?}",
        comparison.availability
    );
    assert_eq!(comparison.role, STRATEGY_ROLE);
    assert_eq!(comparison.reference_label, "Sixty Forty");

    let history_after = common::get_all_snapshots(&db).await;
    assert_eq!(
        history_before.len(),
        history_after.len(),
        "the NAV history must be untouched"
    );
    assert_eq!(history_before, history_after);
    let assets_after = benchmark_blend_component::Entity::find()
        .order_by_asc(benchmark_blend_component::Column::BlendId)
        .all(&db)
        .await
        .expect("the saved definition should be readable");
    assert_eq!(assets_before, assets_after);
}

#[tokio::test]
async fn malformed_persisted_strategy_data_remains_an_error() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let component = benchmark_blend_component::Entity::find()
        .filter(benchmark_blend_component::Column::BlendId.eq(&blend_id))
        .one(&db)
        .await
        .expect("the component should be readable")
        .expect("the component should exist");
    let mut active: benchmark_blend_component::ActiveModel = component.into();
    active.target_weight = Set(0.1);
    active
        .update(&db)
        .await
        .expect("the component should be updatable");

    let failure =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect_err("malformed persisted data must remain an error");
    assert!(
        failure.to_string().contains("total"),
        "the failure must name the broken invariant: {failure}"
    );
}

#[tokio::test]
async fn the_strategy_selection_migration_preserves_accounting_and_cache_history() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("in-memory SQLite");
    let boundary = Migrator::migrations()
        .iter()
        .position(|migration| migration.name().contains("add_strategy_selection"))
        .expect("the strategy selection migration should be registered") as u32;
    Migrator::up(&db, Some(boundary))
        .await
        .expect("the schema before the strategy selection migration");

    for sql in [
        "INSERT INTO assets (id, ticker, name, asset_type, currency, created_at) VALUES (17, 'XFAKEMIG', 'migration fixture', 'stock', 'USD', 'legacy-created-at')",
        "INSERT INTO transactions (id, asset_id, tx_type, date, units, unit_price_cents, fees_cents, created_at) VALUES (23, 17, 'buy', '2024-01-02', 2, 10000, 1, 'ledger-created-at')",
        "INSERT INTO daily_asset_prices (id, asset_id, date, closing_price, is_api_failure) VALUES (31, 17, '2024-01-02', 99.5, 0)",
        "INSERT INTO daily_exchange_rates (from_currency, to_currency, date, rate) VALUES ('USD', 'EUR', '2024-01-02', 0.91)",
        "INSERT INTO portfolio_history (date, cash_balance, asset_value, total_value, outstanding_shares, nav) VALUES ('2024-01-02', 0, 199, 199, 2, 99.5)",
        "INSERT INTO portfolio_asset_history (id, date, asset_id, quantity, closing_price, market_value, exchange_rate) VALUES (41, '2024-01-02', 17, 2, 99.5, 181.09, 0.91)",
        "INSERT INTO benchmark_references (id, display_name, instrument_ticker, source, source_identifier, currency, adjustment_provenance, adjustment_qualification, return_basis_qualification, role, universe, hedge_policy, source_provenance, history_boundary, verification_state) VALUES ('migration-reference', 'Migration reference', 'MIG', 'yahoo_finance', 'MIG', 'USD', 'adjusted', 'split_adjusted', 'total_return', 'Investment universe', 'Equities', 'Unhedged', 'source', 'boundary', 'verified')",
        "INSERT INTO benchmark_reference_prices (reference_id, date, closing_price, is_api_failure) VALUES ('migration-reference', '2024-01-02', 88.0, 0)",
    ] {
        db.execute(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await
            .expect("seed pre-migration row");
    }

    Migrator::up(&db, Some(1))
        .await
        .expect("the additive strategy selection migration");
    Migrator::down(&db, Some(1))
        .await
        .expect("the strategy selection migration should roll back");
    Migrator::up(&db, Some(1))
        .await
        .expect("the strategy selection migration should apply again");

    for (sql, expected) in [
        (
            "SELECT CAST(id AS REAL) AS value FROM assets WHERE ticker = 'XFAKEMIG'",
            17.0,
        ),
        (
            "SELECT CAST(id AS REAL) AS value FROM transactions WHERE id = 23",
            23.0,
        ),
        (
            "SELECT closing_price AS value FROM daily_asset_prices WHERE id = 31",
            99.5,
        ),
        (
            "SELECT rate AS value FROM daily_exchange_rates WHERE from_currency = 'USD'",
            0.91,
        ),
        (
            "SELECT nav AS value FROM portfolio_history WHERE date = '2024-01-02'",
            99.5,
        ),
        (
            "SELECT market_value AS value FROM portfolio_asset_history WHERE id = 41",
            181.09,
        ),
        (
            "SELECT closing_price AS value FROM benchmark_reference_prices WHERE reference_id = 'migration-reference'",
            88.0,
        ),
    ] {
        let value: f64 = db
            .query_one(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await
            .expect("preserved source query")
            .expect("preserved source row")
            .try_get("", "value")
            .expect("preserved source value");
        assert_eq!(value, expected, "preserved value from {sql}");
    }
}

#[tokio::test]
async fn a_selected_strategy_refuses_the_destructive_migration_rollback() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let rollback = Migrator::down(&db, Some(migrations_from_strategy_selection())).await;
    assert!(
        rollback.is_err(),
        "a destructive rollback must be refused while a Strategy benchmark is selected"
    );
    let error = rollback.expect_err("the rollback must fail").to_string();
    assert!(
        error.contains("Strategy benchmark is selected"),
        "the refusal must explain the refusal: {error}"
    );
    assert!(
        strategy::selected_strategy(&db)
            .await
            .expect("the selection should be readable")
            .is_some(),
        "the refused rollback must keep the explicit selection intact"
    );
}

#[tokio::test]
async fn the_database_refuses_a_second_selection_slot() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;

    let refused = benchmark_strategy_selection::ActiveModel {
        selection_slot: Set("another-portfolio".to_owned()),
        reference_id: Set(REFERENCE_A.to_owned()),
        selected_at: Set("2025-02-05T00:00:00Z".to_owned()),
        selection_basis: Set(strategy::SELECTION_BASIS_EXPLICIT.to_owned()),
    }
    .insert(&db)
    .await
    .expect_err("the database must refuse a second selection slot");
    assert!(
        refused.to_string().contains("CHECK"),
        "the refusal must be the singleton constraint: {refused}"
    );

    let rows = benchmark_strategy_selection::Entity::find()
        .all(&db)
        .await
        .expect("the selection table should be readable");
    assert!(
        rows.is_empty(),
        "the refused insert must not leave a row behind"
    );
}

#[tokio::test]
async fn a_corrupt_persisted_reference_price_stays_a_hard_error() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    // A persisted price of zero is corrupt persisted data, not a missing
    // observation: it must never be reported as an unavailable comparison.
    benchmark_reference_price::Entity::update_many()
        .col_expr(
            benchmark_reference_price::Column::ClosingPrice,
            sea_orm::sea_query::Expr::value(0.0),
        )
        .filter(benchmark_reference_price::Column::ReferenceId.eq(REFERENCE_A))
        .exec(&db)
        .await
        .expect("the cached prices should be updatable");

    let failure =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect_err("a corrupt persisted price must remain an error");
    assert!(
        failure
            .to_string()
            .contains("invalid persisted benchmark reference price"),
        "the failure must name the corrupt observation: {failure}"
    );
}

#[tokio::test]
async fn the_strategy_comparison_ends_at_the_ready_nav_end_not_yesterday() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    // The market is one day further on than the portfolio's ready history:
    // the comparison must end at the Effective valuation date, not at
    // yesterday, and it must not require a close the portfolio does not have.
    let today = NaiveDate::from_ymd_opt(2025, 2, 6).expect("valid clock date");
    let sources = stepped_sources();
    // The blend is saved while the last completed close is still the
    // portfolio's Effective valuation date; the market day moves on afterwards.
    let blend_id = save_stepped_blend(&db, &sources, clock()).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let comparison =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect("the comparison should be calculated");
    assert_eq!(
        comparison.availability,
        ComparisonAvailability::Available,
        "the comparison must be available at the Effective valuation date"
    );
    assert_eq!(comparison.end_date.as_deref(), Some(WINDOW_END));
    assert!(
        !comparison
            .reference_wealth
            .iter()
            .any(|point| point.date == "2025-02-05"),
        "the comparison must not require a close after the Effective valuation date"
    );
    // The Effective valuation date is unchanged.
    let latest = common::get_all_snapshots(&db).await;
    assert_eq!(
        latest.last().map(|snapshot| snapshot.date.clone()),
        Some(WINDOW_END.to_owned())
    );
}

#[tokio::test]
async fn a_missing_earlier_close_keeps_the_anchor_and_the_earlier_coverage() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let market_data = market_data_at(&sources, today);
    // A blend admitted from an explicitly selected later start.
    let definition = custom_blends::create_custom_blend(
        &db,
        &market_data,
        BlendCreateRequest {
            name: "Later Start".to_owned(),
            components: vec![rstock::models::BlendComponentArgument {
                identity: REFERENCE_A.to_owned(),
                weight_percent: 100.0,
            }],
            start_date: Some(
                NaiveDate::parse_from_str("2025-01-16", "%Y-%m-%d").expect("valid start date"),
            ),
        },
    )
    .await
    .expect("the blend should be saved");
    insert_invested_portfolio(&db).await;
    strategy::select_strategy(&db, &market_data, &definition.id)
        .await
        .expect("the strategy should be selected");

    // The strategy's own component loses a required close, so a later
    // required input is missing.
    benchmark_reference_price::Entity::delete_many()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(REFERENCE_A))
        .filter(benchmark_reference_price::Column::Date.eq("2025-02-04"))
        .exec(&db)
        .await
        .expect("the cached prices should be removable");
    let gapped_market_data =
        market_data_at(&sources_without(&sources, "XREFA", "2025-02-04"), today);

    let comparison = strategy::compute_strategy_comparison(
        &db,
        &gapped_market_data,
        &ready_snapshots(&db).await,
    )
    .await
    .expect("a missing close must not fail the command");
    assert!(
        matches!(
            comparison.availability,
            ComparisonAvailability::Unavailable { .. }
        ),
        "the comparison must be unavailable, got {:?}",
        comparison.availability
    );
    // The known anchor and the earlier coverage stay visible.
    assert_eq!(
        comparison.strategy_anchor_date.as_deref(),
        Some("2025-01-16"),
        "the known anchor must survive the failed preparation"
    );
    let limitation = comparison
        .earlier_period_limitation
        .as_ref()
        .expect("the earlier coverage must survive the failed preparation");
    assert!(
        limitation.contains("2025-01-16"),
        "the earlier coverage must name the anchor: {limitation}"
    );
    assert_eq!(
        comparison.reference_id.as_deref(),
        Some(definition.id.as_str())
    );
    assert_eq!(comparison.components.len(), 1);
    assert_eq!(comparison.reset_rule.as_deref(), Some("monthly"));
    // The standard windows stay visibly unavailable.
    assert_eq!(comparison.standard_periods.len(), 5);
    for period in &comparison.standard_periods {
        assert!(
            matches!(
                period.subject_return_pct,
                ComparisonFact::Unavailable { .. }
            ),
            "the standard window must stay unavailable"
        );
    }
}

#[tokio::test]
async fn a_component_interior_weekday_gap_leaves_the_comparison_unavailable() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    // The component's own series loses an interior completed weekday: the
    // cached close disappears and the source no longer reports it.
    benchmark_reference_price::Entity::delete_many()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(REFERENCE_A))
        .filter(benchmark_reference_price::Column::Date.eq("2025-01-20"))
        .exec(&db)
        .await
        .expect("the cached prices should be removable");
    let gapped_market_data =
        market_data_at(&sources_without(&sources, "XREFA", "2025-01-20"), today);

    let comparison = strategy::compute_strategy_comparison(
        &db,
        &gapped_market_data,
        &ready_snapshots(&db).await,
    )
    .await
    .expect("a missing component close must not fail the command");
    let reason = match &comparison.availability {
        ComparisonAvailability::Unavailable { reason } => reason.clone(),
        other => panic!("the comparison must be unavailable, got {other:?}"),
    };
    assert!(
        reason.contains("2025-01-20"),
        "the reason must name the missing component close: {reason}"
    );
    assert!(
        comparison.subject_return_pct.is_none(),
        "the measured return must not be silently shortened"
    );
}

#[tokio::test]
async fn replacing_and_deleting_the_old_strategy_preserves_the_history() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = stepped_sources();
    let today = clock();
    let first = save_stepped_blend(&db, &sources, today).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &first)
        .await
        .expect("the first strategy should be selected");

    let second = custom_blends::create_custom_blend(
        &db,
        &market_data,
        BlendCreateRequest {
            name: "All Equity".to_owned(),
            components: vec![rstock::models::BlendComponentArgument {
                identity: REFERENCE_B.to_owned(),
                weight_percent: 100.0,
            }],
            start_date: Some(
                NaiveDate::parse_from_str(WINDOW_START, "%Y-%m-%d").expect("valid start date"),
            ),
        },
    )
    .await
    .expect("the second blend should be saved");
    strategy::select_strategy(&db, &market_data, &second.id)
        .await
        .expect("the second strategy should be selected");

    // The replaced definition is deletable again.
    let deletion = custom_blends::delete_custom_blend(&db, &first)
        .await
        .expect("a replaced definition should be deletable");
    assert!(deletion.deleted);
    // The historical market data and the NAV history survive the deletion.
    let cached_prices = benchmark_reference_price::Entity::find()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(REFERENCE_A))
        .all(&db)
        .await
        .expect("the cached prices should be readable");
    assert!(
        !cached_prices.is_empty(),
        "the deletion must preserve the historical market data"
    );
    let snapshots = common::get_all_snapshots(&db).await;
    assert!(
        !snapshots.is_empty(),
        "the deletion must preserve the NAV history"
    );
    assert_eq!(
        snapshots.last().map(|snapshot| snapshot.date.clone()),
        Some(WINDOW_END.to_owned())
    );
}

#[tokio::test]
async fn the_selection_commands_persist_through_the_public_dispatch() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = stepped_sources();
    let today = clock();
    let first = save_stepped_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    let second = custom_blends::create_custom_blend(
        &db,
        &market_data,
        BlendCreateRequest {
            name: "All Equity".to_owned(),
            components: vec![rstock::models::BlendComponentArgument {
                identity: REFERENCE_B.to_owned(),
                weight_percent: 100.0,
            }],
            start_date: Some(
                NaiveDate::parse_from_str(WINDOW_START, "%Y-%m-%d").expect("valid start date"),
            ),
        },
    )
    .await
    .expect("the second blend should be saved");

    let selected = run_strategy(
        &db,
        &sources,
        today,
        &["rstock", "strategy", "select", &first],
        OutputFormat::Json,
    )
    .await
    .expect("the strategy should be selected");
    let selected = common::parse_json_envelope(&selected);
    assert_eq!(selected["command"], "strategy.select");
    assert_eq!(selected["data"]["selected"], true);
    assert_eq!(selected["data"]["reference_id"], first.as_str());

    let shown = run_strategy(
        &db,
        &sources,
        today,
        &["rstock", "strategy", "show"],
        OutputFormat::Json,
    )
    .await
    .expect("the selection should be shown");
    let shown = common::parse_json_envelope(&shown);
    assert_eq!(shown["data"]["reference_id"], first.as_str());
    assert_eq!(
        strategy::selected_strategy(&db)
            .await
            .expect("the selection should be readable")
            .expect("a strategy should be selected")
            .reference_id,
        first
    );

    // A later selection replaces the earlier one through the same dispatch.
    let replaced = run_strategy(
        &db,
        &sources,
        today,
        &["rstock", "strategy", "select", &second.id],
        OutputFormat::Json,
    )
    .await
    .expect("the second strategy should be selected");
    let replaced = common::parse_json_envelope(&replaced);
    assert_eq!(replaced["data"]["reference_id"], second.id.as_str());
    assert_eq!(replaced["data"]["replaced_reference_id"], first.as_str());
    assert_eq!(
        strategy::selected_strategy(&db)
            .await
            .expect("the selection should be readable")
            .expect("a strategy should be selected")
            .reference_id,
        second.id
    );

    let unselected = run_strategy(
        &db,
        &sources,
        today,
        &["rstock", "strategy", "unselect"],
        OutputFormat::Json,
    )
    .await
    .expect("the strategy should be unselected");
    let unselected = common::parse_json_envelope(&unselected);
    assert_eq!(unselected["data"]["selected"], false);
    assert!(strategy::selected_strategy(&db)
        .await
        .expect("the selection should be readable")
        .is_none());
}

#[tokio::test]
async fn a_selected_strategy_blocks_the_deletion_through_the_public_dispatch() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let mut output = Vec::new();
    let failure = rstock::cli::run_blend_command_with_writer(
        &db,
        &market_data,
        rstock::cli::BlendCommands::Delete {
            blend_id: blend_id.clone(),
        },
        OutputFormat::Json,
        &mut output,
    )
    .await
    .expect_err("a selected strategy must not be deletable");
    assert!(
        failure.to_string().contains("Strategy benchmark"),
        "the failure must name the Strategy benchmark: {failure}"
    );
    assert!(
        custom_blends::show_custom_blend(&db, &blend_id)
            .await
            .is_ok(),
        "the rejected deletion must leave the definition saved"
    );

    strategy::unselect_strategy(&db)
        .await
        .expect("the strategy should be unselected");
    let mut output = Vec::new();
    rstock::cli::run_blend_command_with_writer(
        &db,
        &market_data,
        rstock::cli::BlendCommands::Delete {
            blend_id: blend_id.clone(),
        },
        OutputFormat::Json,
        &mut output,
    )
    .await
    .expect("an unused definition should be deletable");
    let deletion = common::parse_json_envelope(&output);
    assert_eq!(deletion["data"]["deleted"], true);
    assert!(
        custom_blends::show_custom_blend(&db, &blend_id)
            .await
            .is_err(),
        "the deleted definition must be gone"
    );
}

#[tokio::test]
async fn a_definition_that_is_not_a_saved_custom_blend_cannot_be_selected() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let market_data = market_data_at(&sources, today);

    let failure = strategy::select_strategy(&db, &market_data, REFERENCE_A)
        .await
        .expect_err("a plain reference is not a Strategy benchmark");
    assert!(
        failure.to_string().contains("saved Custom blend"),
        "the failure must name the required definition kind: {failure}"
    );
    assert!(
        strategy::selected_strategy(&db)
            .await
            .expect("the selection should be readable")
            .is_none(),
        "a rejected selection must not persist"
    );
}

#[tokio::test]
async fn a_missing_interior_weekday_leaves_the_strategy_comparison_unavailable() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;
    // The portfolio's ready NAV history misses one completed weekday.
    let asset_id = common::insert_asset(&db, "XFAKE1", "Invested fixture", "stock", "EUR").await;
    let mut nav = 100.0;
    for (date, _) in weekday_observations(WINDOW_START, WINDOW_END, |_| 0.0) {
        if date != "2025-01-20" {
            common::insert_portfolio_snapshot(&db, &date, nav, 10.0).await;
            common::insert_portfolio_asset_snapshot(
                &db,
                &date,
                asset_id,
                10.0,
                nav,
                nav * 10.0,
                1.0,
            )
            .await;
        }
        nav += 1.0;
    }
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let comparison =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect("the comparison should be reported");
    assert!(
        matches!(
            comparison.availability,
            ComparisonAvailability::Unavailable { .. }
        ),
        "a missing completed weekday must leave the comparison unavailable, got {:?}",
        comparison.availability
    );
    let reason = match &comparison.availability {
        ComparisonAvailability::Unavailable { reason } => reason.clone(),
        other => panic!("the comparison must be unavailable, got {other:?}"),
    };
    assert!(
        reason.contains("2025-01-20"),
        "the reason must name the missing close: {reason}"
    );
    assert!(
        comparison.subject_return_pct.is_none(),
        "the measured return must not be silently shortened"
    );
}

#[tokio::test]
async fn the_strategy_commands_report_the_same_selection_in_human_and_json() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let today = clock();
    let blend_id = save_stepped_blend(&db, &sources, today).await;

    let json = run_strategy(
        &db,
        &sources,
        today,
        &["rstock", "strategy", "select", &blend_id],
        OutputFormat::Json,
    )
    .await
    .expect("the strategy should be selected");
    let envelope = common::parse_json_envelope(&json);
    assert_eq!(envelope["command"], "strategy.select");
    assert_eq!(envelope["data"]["selected"], true);
    assert_eq!(envelope["data"]["reference_id"], blend_id.as_str());
    assert_eq!(envelope["data"]["display_name"], "Sixty Forty");
    assert_eq!(
        envelope["data"]["selection_basis"],
        strategy::SELECTION_BASIS_EXPLICIT
    );
    assert_eq!(envelope["data"]["selected_at"], "2025-02-05T00:00:00Z");

    let human = run_strategy(
        &db,
        &sources,
        today,
        &["rstock", "strategy", "show"],
        OutputFormat::Human,
    )
    .await
    .expect("the selection should render");
    let human = String::from_utf8(human).unwrap();
    for expected in [
        "Selected Strategy benchmark 'Sixty Forty'",
        "Selected at: 05-02-2025",
        "basis: explicit_user_selection",
        "compared independently of the portfolio's actual allocation",
    ] {
        assert!(
            human.contains(expected),
            "the human output omitted {expected}: {human}"
        );
    }
    assert_no_iso_dates(&human);

    let shown = run_strategy(
        &db,
        &sources,
        today,
        &["rstock", "strategy", "show"],
        OutputFormat::Json,
    )
    .await
    .expect("the selection should be shown");
    let shown = common::parse_json_envelope(&shown);
    assert_eq!(shown["command"], "strategy.show");
    assert_eq!(shown["data"]["selected"], true);
    assert_eq!(shown["data"]["reference_id"], blend_id.as_str());
    assert_eq!(shown["data"]["display_name"], "Sixty Forty");

    let unselected = run_strategy(
        &db,
        &sources,
        today,
        &["rstock", "strategy", "unselect"],
        OutputFormat::Json,
    )
    .await
    .expect("the strategy should be unselected");
    let unselected = common::parse_json_envelope(&unselected);
    assert_eq!(unselected["command"], "strategy.unselect");
    assert_eq!(unselected["data"]["selected"], false);
    assert_eq!(unselected["data"]["reference_id"], serde_json::Value::Null);
    assert_eq!(
        unselected["data"]["replaced_reference_id"],
        blend_id.as_str()
    );
}

/// The position of one invalid value inside the ready NAV history.
#[derive(Clone, Copy)]
enum InvalidNavPosition {
    First,
    Interior,
    Last,
}

impl InvalidNavPosition {
    /// The name of the position, for the failure message.
    fn label(self) -> &'static str {
        match self {
            Self::First => "the first close",
            Self::Interior => "an interior close",
            Self::Last => "the current close",
        }
    }
}

/// The Strategy comparison of a portfolio whose ready NAV holds an invalid
/// value at one position.
///
/// The invalid value is written into the ready history the comparison reads,
/// so the comparison observes exactly the corruption the portfolio holds.
async fn comparison_with_an_invalid_ready_nav(
    invalid: f64,
    position: InvalidNavPosition,
) -> anyhow::Result<rstock::models::StrategyComparison> {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let blend_id = save_stepped_blend(&db, &sources, clock()).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, clock());
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");
    let mut snapshots = ready_snapshots(&db).await;
    let index = match position {
        InvalidNavPosition::First => 0,
        InvalidNavPosition::Interior => snapshots.len() / 2,
        InvalidNavPosition::Last => snapshots.len() - 1,
    };
    snapshots[index].nav = invalid;
    strategy::compute_strategy_comparison(&db, &market_data, &snapshots).await
}

#[tokio::test]
async fn an_invalid_ready_nav_is_a_hard_error_and_not_a_coverage_gap() {
    for (invalid, label) in [
        (0.0, "a zero NAV"),
        (-1.0, "a negative NAV"),
        (f64::NAN, "a non-finite NAV"),
    ] {
        for position in [
            InvalidNavPosition::First,
            InvalidNavPosition::Interior,
            InvalidNavPosition::Last,
        ] {
            let failure = comparison_with_an_invalid_ready_nav(invalid, position)
                .await
                .expect_err(&format!(
                    "{label} at {} must be a hard error",
                    position.label()
                ));
            assert!(
                failure.to_string().contains("invalid fund EUR observation"),
                "{label} at {} must be reported as corrupt data, not as a coverage gap: {failure}",
                position.label()
            );
        }
    }
}

/// The clock of the later-start fixture.
///
/// The blend is admitted from an explicit start, so the market day is later
/// than the portfolio's ready history.
fn later_clock() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 2, 14).expect("valid clock date")
}

/// The source of the later-start fixture: one component that holds every
/// close of the later admission window.
fn later_start_sources() -> MockMarketDataSources {
    offline_sources(
        weekday_observations("2025-02-10", "2025-02-13", |_| 100.0),
        weekday_observations("2025-02-10", "2025-02-13", |_| 100.0),
    )
}

/// Saves one Custom blend that is admitted from an explicit later start.
async fn save_later_start_blend(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
    today: NaiveDate,
    start: &str,
) -> String {
    let market_data = market_data_at(sources, today);
    let definition = custom_blends::create_custom_blend(
        db,
        &market_data,
        BlendCreateRequest {
            name: "Later Start".to_owned(),
            components: vec![
                rstock::models::BlendComponentArgument {
                    identity: REFERENCE_A.to_owned(),
                    weight_percent: 60.0,
                },
                rstock::models::BlendComponentArgument {
                    identity: "cash".to_owned(),
                    weight_percent: 40.0,
                },
            ],
            start_date: Some(
                NaiveDate::parse_from_str(start, "%Y-%m-%d").expect("valid start date"),
            ),
        },
    )
    .await
    .expect("the blend should be saved");
    definition.id
}

/// The portfolio fixture whose ready history ends before the given date.
///
/// The persisted asset value matches the persisted per-asset exposures, so
/// the Portfolio view accepts the history as consistent.
async fn insert_invested_portfolio_until(db: &DatabaseConnection, end: &str) {
    let asset_id = common::insert_asset(db, "XFAKE1", "Invested fixture", "stock", "EUR").await;
    let mut nav = 100.0;
    for (date, _) in weekday_observations(WINDOW_START, end, |_| 0.0) {
        common::insert_portfolio_snapshot_with_values(db, &date, nav * 10.0, nav * 10.0, nav, 10.0)
            .await;
        common::insert_portfolio_asset_snapshot(db, &date, asset_id, 10.0, nav, nav * 10.0, 1.0)
            .await;
        nav += 1.0;
    }
}

/// The ready Complete NAV snapshots of the fixture up to the given date.
async fn ready_snapshots_until(db: &DatabaseConnection, end: &str) -> Vec<PortfolioSnapshot> {
    portfolio_history_repo::find_between(db, WINDOW_START, end)
        .await
        .expect("the ready NAV history should be readable")
}

#[tokio::test]
async fn a_strategy_anchor_after_the_ready_history_is_an_unavailable_comparison() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = later_start_sources();
    let blend_id = save_later_start_blend(&db, &sources, later_clock(), "2025-02-10").await;
    // The portfolio's ready history ends before the selected anchor: a
    // backdated ledger entry or an unavailable holding price can leave the
    // ready history older than the selected Strategy benchmark.
    insert_invested_portfolio_until(&db, "2025-01-31").await;
    let market_data = market_data_at(&sources, later_clock());
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let comparison = strategy::compute_strategy_comparison(
        &db,
        &market_data,
        &ready_snapshots_until(&db, "2025-01-31").await,
    )
    .await
    .expect("a non-overlapping window must not fail the command");

    assert!(
        matches!(
            comparison.availability,
            ComparisonAvailability::Unavailable { .. }
        ),
        "the comparison must be unavailable, got {:?}",
        comparison.availability
    );
    assert_eq!(
        comparison.strategy_anchor_date.as_deref(),
        Some("2025-02-10"),
        "the known anchor must survive the non-overlapping window"
    );
    assert_eq!(comparison.reference_id.as_deref(), Some(blend_id.as_str()));
    assert_eq!(comparison.reference_label, "Later Start");
    assert_eq!(comparison.definition_kind.as_deref(), Some("custom_blend"));
    assert!(
        comparison
            .earlier_period_limitation
            .as_deref()
            .unwrap_or_default()
            .contains("2025-02-10"),
        "the earlier coverage must stay visible: {:?}",
        comparison.earlier_period_limitation
    );
    assert_eq!(
        comparison.standard_periods.len(),
        5,
        "every standard window must stay visible"
    );
    for period in &comparison.standard_periods {
        assert!(
            matches!(
                period.subject_return_pct,
                ComparisonFact::Unavailable { .. }
            ),
            "every standard window must stay visibly unavailable"
        );
    }
    // The window is never silently shortened: no measured window is reported.
    assert_eq!(comparison.start_date, None);
    assert_eq!(comparison.end_date, None);
    assert!(comparison.subject_wealth.is_empty());
    assert!(comparison.reference_wealth.is_empty());
}

#[tokio::test]
async fn a_malformed_component_price_date_is_a_hard_error() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let blend_id = save_stepped_blend(&db, &sources, clock()).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, clock());
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    // A persisted component price whose stored date is malformed is corrupt
    // persisted data, not a missing observation.
    benchmark_reference_price::ActiveModel {
        id: sea_orm::NotSet,
        reference_id: Set(REFERENCE_A.to_owned()),
        date: Set("2025-01-16x".to_owned()),
        closing_price: Set(100.0),
        is_api_failure: Set(false),
        is_source_observation: Set(true),
    }
    .insert(&db)
    .await
    .expect("the corrupt cached price should persist");

    let failure =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect_err("a malformed persisted component date must be a hard error");
    assert!(
        failure
            .to_string()
            .contains("malformed persisted benchmark reference price date"),
        "the failure must name the corrupt observation: {failure}"
    );
}

#[tokio::test]
async fn a_corrupt_ready_nav_fails_the_portfolio_view_instead_of_hiding() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let blend_id = save_stepped_blend(&db, &sources, clock()).await;
    insert_invested_portfolio_until(&db, WINDOW_END).await;
    let market_data = market_data_at(&sources, clock());
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    // A persisted NAV that is not a positive number is corrupt persisted
    // data: the Portfolio view must report it instead of presenting an
    // unavailable comparison.
    portfolio_history::Entity::update_many()
        .col_expr(
            portfolio_history::Column::Nav,
            sea_orm::sea_query::Expr::value(0.0),
        )
        .filter(portfolio_history::Column::Date.eq("2025-01-16"))
        .exec(&db)
        .await
        .expect("the persisted NAV should be updatable");

    let failure = rstock::services::portfolio_view::get_portfolio_view(
        &db,
        &market_data,
        rstock::models::NavHistoryRequest::All,
    )
    .await
    .expect_err("a corrupt persisted NAV must fail the Portfolio view");
    assert!(
        failure.to_string().contains("invalid fund EUR observation"),
        "the failure must name the corrupt observation: {failure}"
    );
}

#[tokio::test]
async fn the_portfolio_view_keeps_its_facts_when_the_strategy_anchor_is_after_the_ready_history() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = later_start_sources();
    let blend_id = save_later_start_blend(&db, &sources, later_clock(), "2025-02-10").await;
    // The portfolio's ready history ends before the selected anchor.
    insert_invested_portfolio_until(&db, "2025-01-31").await;
    let market_data = market_data_at(&sources, later_clock());
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let view = rstock::services::portfolio_view::get_portfolio_view(
        &db,
        &market_data,
        rstock::models::NavHistoryRequest::All,
    )
    .await
    .expect("an older usable portfolio history must not fail the Portfolio view");

    // The Strategy comparison is unavailable and keeps what is still known.
    assert!(
        matches!(
            view.strategy_benchmark.availability,
            ComparisonAvailability::Unavailable { .. }
        ),
        "the strategy comparison must be unavailable, got {:?}",
        view.strategy_benchmark.availability
    );
    assert_eq!(
        view.strategy_benchmark.strategy_anchor_date.as_deref(),
        Some("2025-02-10"),
        "the known anchor must survive the non-overlapping window"
    );
    assert_eq!(
        view.strategy_benchmark.reference_id.as_deref(),
        Some(blend_id.as_str())
    );
    assert!(
        view.strategy_benchmark
            .earlier_period_limitation
            .as_deref()
            .unwrap_or_default()
            .contains("2025-02-10"),
        "the earlier coverage must stay visible"
    );
    assert_eq!(view.strategy_benchmark.standard_periods.len(), 5);
    for period in &view.strategy_benchmark.standard_periods {
        assert!(
            matches!(
                period.subject_return_pct,
                ComparisonFact::Unavailable { .. }
            ),
            "every standard window must stay visibly unavailable"
        );
    }
    // The window is never silently shortened.
    assert_eq!(view.strategy_benchmark.start_date, None);
    assert_eq!(view.strategy_benchmark.end_date, None);

    // The older usable portfolio facts stay intact and independent.
    let rstock::models::PortfolioPerformance::Available(performance) = &view.performance else {
        panic!("the older ready history must stay measurable");
    };
    assert_eq!(
        performance.effective_valuation_date, "2025-01-31",
        "the Effective valuation date stays the older usable history"
    );
    assert_eq!(performance.inception_date, WINDOW_START);
    // The exposure-matched primary comparison keeps its own availability: the
    // fixture assigns no primary reference, so it is unavailable for that own
    // reason and never because of the Strategy benchmark.
    if let ComparisonAvailability::Unavailable { reason } = &view.exposure_benchmark.availability {
        assert!(
            reason.contains("no explicit primary reference assignment"),
            "the exposure comparison must not fail because of the Strategy benchmark: {reason}"
        );
    } else {
        panic!(
            "the exposure comparison keeps its own availability, got {:?}",
            view.exposure_benchmark.availability
        );
    }
}

#[tokio::test]
async fn an_invalid_persisted_fx_rate_is_a_hard_error() {
    let db = setup_test_db().await;
    insert_verified_reference_in(&db, REFERENCE_A, "XREFA", "USD").await;
    let sources = foreign_currency_sources();
    let blend_id = save_stepped_blend(&db, &sources, clock()).await;
    insert_invested_portfolio(&db).await;
    let market_data = market_data_at(&sources, clock());
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    // A persisted FX rate that is not a positive number is corrupt persisted
    // data, not a missing observation.
    daily_exchange_rate::Entity::update_many()
        .col_expr(
            daily_exchange_rate::Column::Rate,
            sea_orm::sea_query::Expr::value(0.0),
        )
        .filter(daily_exchange_rate::Column::FromCurrency.eq("USD"))
        .filter(daily_exchange_rate::Column::ToCurrency.eq("EUR"))
        .filter(daily_exchange_rate::Column::Date.eq("2025-01-16"))
        .exec(&db)
        .await
        .expect("the persisted exchange rates should be updatable");

    let failure =
        strategy::compute_strategy_comparison(&db, &market_data, &ready_snapshots(&db).await)
            .await
            .expect_err("an invalid persisted FX rate must be a hard error");
    assert!(
        failure.to_string().contains("invalid persisted FX rate"),
        "the failure must name the corrupt observation: {failure}"
    );
}

#[tokio::test]
async fn a_malformed_component_price_date_fails_the_portfolio_view() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let blend_id = save_stepped_blend(&db, &sources, clock()).await;
    insert_invested_portfolio_until(&db, WINDOW_END).await;
    let market_data = market_data_at(&sources, clock());
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    // A persisted component price whose stored date is malformed is corrupt
    // persisted data: the Portfolio view must report it.
    benchmark_reference_price::ActiveModel {
        id: sea_orm::NotSet,
        reference_id: Set(REFERENCE_A.to_owned()),
        date: Set("2025-01-16x".to_owned()),
        closing_price: Set(100.0),
        is_api_failure: Set(false),
        is_source_observation: Set(true),
    }
    .insert(&db)
    .await
    .expect("the corrupt cached price should persist");

    let failure = rstock::services::portfolio_view::get_portfolio_view(
        &db,
        &market_data,
        rstock::models::NavHistoryRequest::All,
    )
    .await
    .expect_err("a malformed persisted component date must fail the Portfolio view");
    assert!(
        failure
            .to_string()
            .contains("malformed persisted benchmark reference price date"),
        "the failure must name the corrupt observation: {failure}"
    );
}

#[tokio::test]
async fn the_strategy_show_command_reports_a_failed_selected_definition_lookup() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = stepped_sources();
    let blend_id = save_stepped_blend(&db, &sources, clock()).await;
    let market_data = market_data_at(&sources, clock());
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    // The database's foreign key refuses to delete a selected definition, so
    // the corrupted selection is simulated by bypassing that protection: the
    // selected definition is gone while the selection still names it.
    db.execute_unprepared("PRAGMA foreign_keys = OFF")
        .await
        .expect("the foreign key protection should be disabled");
    benchmark_reference::Entity::delete_by_id(blend_id.clone())
        .exec(&db)
        .await
        .expect("the selected definition should be removable");
    db.execute_unprepared("PRAGMA foreign_keys = ON")
        .await
        .expect("the foreign key protection should be restored");

    let failure = run_strategy(
        &db,
        &sources,
        clock(),
        &["rstock", "strategy", "show"],
        OutputFormat::Human,
    )
    .await
    .expect_err("a missing selected definition must not be reported as an empty selection");
    assert!(
        failure
            .to_string()
            .contains("is not a known benchmark definition"),
        "the failure must name the missing selected definition: {failure}"
    );
}
