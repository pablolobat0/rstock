//! Selected Strategy benchmark dashboard seam tests.
//!
//! The Portfolio dashboard shows the explicitly selected Strategy benchmark
//! next to the exposure-matched primary comparison. The fixtures are hand
//! calculated: every expected return, anchor, and reset comes from the
//! persisted Complete NAV snapshots and the fake reference observations.

#![allow(dead_code)]

mod common;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use clap::Parser;
use rstock::{
    cli::{adapters, output::OutputFormat, Cli},
    db::entities::{
        benchmark_assignment, benchmark_reference, benchmark_reference_price,
        portfolio_asset_history, portfolio_history,
    },
    models::{ComparisonAvailability, NavHistoryRequest, PortfolioView},
    services::{
        custom_blends::{self, BlendCreateRequest},
        market_data::SeriesQualification,
        strategy,
    },
};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use common::{market_data_at, setup_test_db, MockMarketDataSources};

const EXPOSURE_REFERENCE: &str = "x-dashboard-exposure-reference";
const STRATEGY_REFERENCE: &str = "x-dashboard-strategy-reference";
const SOURCE: &str = "test_source";
const WINDOW_START: &str = "2025-01-02";
const WINDOW_END: &str = "2025-02-04";

/// Asserts that a calculated value is within tolerance of its expectation.
fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "{actual} is not within tolerance of {expected}"
    );
}

/// The clock the fixtures use.
fn clock() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date")
}

/// The weekday dates of the fixture window.
fn window_dates() -> Vec<String> {
    let mut day = NaiveDate::parse_from_str(WINDOW_START, "%Y-%m-%d").expect("valid start date");
    let last = NaiveDate::parse_from_str(WINDOW_END, "%Y-%m-%d").expect("valid end date");
    let mut dates = Vec::new();
    while day <= last {
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            dates.push(day.format("%Y-%m-%d").to_string());
        }
        day += Duration::days(1);
    }
    dates
}

/// A verified reference definition.
async fn insert_reference(db: &DatabaseConnection, id: &str, name: &str, identifier: &str) {
    benchmark_reference::ActiveModel {
        id: Set(id.to_owned()),
        display_name: Set(name.to_owned()),
        instrument_ticker: Set(identifier.to_owned()),
        source: Set(SOURCE.to_owned()),
        source_identifier: Set(identifier.to_owned()),
        currency: Set("EUR".to_owned()),
        adjustment_provenance: Set("explicit fake total-return observations".to_owned()),
        adjustment_qualification: Set("dividends_reinvested_once".to_owned()),
        return_basis_qualification: Set("total_return".to_owned()),
        role: Set("Investment universe".to_owned()),
        universe: Set("Equities".to_owned()),
        hedge_policy: Set("Unhedged".to_owned()),
        source_provenance: Set("deterministic test source".to_owned()),
        history_boundary: Set("2025-01-02 exact fixture class".to_owned()),
        verification_state: Set("verified".to_owned()),
        official_comparator: Set(None),
        objective_comparator: Set(None),
        kind: Set("reference".to_owned()),
    }
    .insert(db)
    .await
    .expect("the reference definition should persist");
}

/// The verified daily qualification of a fake reference.
fn qualification() -> SeriesQualification {
    SeriesQualification {
        currency: "EUR".to_owned(),
        return_basis: "total_return".to_owned(),
        adjustment: "dividends_reinvested_once".to_owned(),
        hedge_policy: "Unhedged".to_owned(),
        cadence: "daily".to_owned(),
    }
}

/// The persisted Complete NAV history of the fixture.
///
/// The NAV rises by one point per weekday from 100.0 on 02-01-2025 to 123.0
/// on 04-02-2025, so the portfolio's cumulative return over the window is
/// exactly 23%. The ten outstanding shares keep the snapshots unitized.
async fn insert_nav_history(db: &DatabaseConnection, asset_id: i32) {
    for (index, date) in window_dates().into_iter().enumerate() {
        let nav = 100.0 + index as f64;
        portfolio_history::ActiveModel {
            date: Set(date.clone()),
            asset_value: Set(nav * 10.0),
            total_value: Set(nav * 10.0),
            outstanding_shares: Set(10.0),
            nav: Set(nav),
        }
        .insert(db)
        .await
        .expect("the NAV snapshot should persist");
        portfolio_asset_history::ActiveModel {
            date: Set(date),
            asset_id: Set(asset_id),
            quantity: Set(10.0),
            closing_price: Set(nav),
            market_value: Set(nav * 10.0),
            exchange_rate: Set(1.0),
            ..Default::default()
        }
        .insert(db)
        .await
        .expect("the per-asset exposure should persist");
    }
}

/// The offline fixture: a flat exposure reference and a stepped strategy
/// component.
fn dashboard_sources() -> MockMarketDataSources {
    let mut sources = MockMarketDataSources::new();
    let mut exposure = Vec::new();
    let mut strategy_component = Vec::new();
    for date in window_dates() {
        exposure.push((date.clone(), 100.0));
        let price = if date.as_str() >= "2025-01-16" {
            110.0
        } else {
            100.0
        };
        strategy_component.push((date, price));
    }
    sources
        .historical_prices
        .insert("XEXPO".to_owned(), exposure);
    sources
        .historical_prices
        .insert("XSTRAT".to_owned(), strategy_component);
    sources
        .series_qualifications
        .insert((SOURCE.to_owned(), "XEXPO".to_owned()), qualification());
    sources
        .series_qualifications
        .insert((SOURCE.to_owned(), "XSTRAT".to_owned()), qualification());
    sources
}

/// The complete dashboard fixture: the portfolio, its exposure reference, and
/// the saved Custom blend the owner can select as a Strategy benchmark.
async fn dashboard_fixture() -> (DatabaseConnection, MockMarketDataSources) {
    let db = setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKE1", "Dashboard Stock", "stock", "EUR").await;
    insert_reference(&db, EXPOSURE_REFERENCE, "Verified Exposure", "XEXPO").await;
    insert_reference(&db, STRATEGY_REFERENCE, "Verified Strategy", "XSTRAT").await;
    benchmark_assignment::ActiveModel {
        subject_type: Set("tracked".to_owned()),
        subject_id: Set("XFAKE1".to_owned()),
        reference_id: Set(EXPOSURE_REFERENCE.to_owned()),
        is_override: Set(false),
        assigned_at: Set("2025-02-01T00:00:00Z".to_owned()),
    }
    .insert(&db)
    .await
    .expect("the assignment should persist");
    insert_nav_history(&db, asset_id).await;
    (db, dashboard_sources())
}

/// Saves the Custom blend the owner selects as the Strategy benchmark.
///
/// The blend holds 60% of the strategy component and 40% synthetic cash, so
/// its wealth rises by exactly 6% on 16-01-2025 and stays flat afterwards.
async fn save_strategy_blend(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
    today: NaiveDate,
) -> String {
    let market_data = market_data_at(sources, today);
    let definition = custom_blends::create_custom_blend(
        db,
        &market_data,
        BlendCreateRequest {
            name: "Global Sixty Forty".to_owned(),
            components: vec![
                rstock::models::BlendComponentArgument {
                    identity: STRATEGY_REFERENCE.to_owned(),
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

/// The Portfolio view of the fixture.
async fn portfolio_view(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
    today: NaiveDate,
) -> PortfolioView {
    let market_data = market_data_at(sources, today);
    rstock::services::portfolio_view::get_portfolio_view(db, &market_data, NavHistoryRequest::All)
        .await
        .expect("the Portfolio view should be composed")
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

/// The section of the human output that follows a heading.
fn section_after<'a>(text: &'a str, heading: &str) -> &'a str {
    let start = text
        .find(heading)
        .unwrap_or_else(|| panic!("the output has no {heading} section"))
        + heading.len();
    let rest = &text[start..];
    let end = rest.find("\n\n").unwrap_or_else(|| rest.len());
    &rest[..end]
}

/// Runs one dashboard alias through the library-owned production dispatch.
async fn run_dashboard(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
    today: NaiveDate,
    args: &[&str],
    format: OutputFormat,
) -> Vec<u8> {
    let market_data = market_data_at(sources, today);
    let command = Cli::try_parse_from(args)
        .expect("the dashboard command should parse")
        .command;
    let mut output = Vec::new();
    rstock::cli::run_command_with_writer(db, &market_data, command, format, &mut output)
        .await
        .expect("the dashboard should render");
    output
}

#[tokio::test]
async fn the_dashboard_shows_the_selected_strategy_beside_the_exposure_comparison() {
    let (db, sources) = dashboard_fixture().await;
    let today = clock();
    let blend_id = save_strategy_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let view = portfolio_view(&db, &sources, today).await;
    assert_eq!(
        view.exposure_benchmark.role, "primary",
        "the exposure comparison keeps its own role"
    );
    assert_eq!(view.strategy_benchmark.role, "strategy");
    assert_eq!(
        view.strategy_benchmark.method,
        "explicit_strategy_selection"
    );
    assert_eq!(
        view.strategy_benchmark.availability,
        ComparisonAvailability::Available
    );
    assert_eq!(
        view.strategy_benchmark.reference_label,
        "Global Sixty Forty"
    );
    assert_eq!(
        view.strategy_benchmark.start_date.as_deref(),
        Some(WINDOW_START)
    );
    assert_eq!(
        view.strategy_benchmark.end_date.as_deref(),
        Some(WINDOW_END)
    );
    assert_close(
        view.strategy_benchmark
            .subject_return_pct
            .expect("a portfolio return"),
        23.0,
    );
    assert_close(
        view.strategy_benchmark
            .reference_return_pct
            .expect("a strategy return"),
        6.0,
    );
    assert_close(
        view.strategy_benchmark
            .excess_return_percentage_points
            .expect("an excess return"),
        17.0,
    );

    // The exposure comparison is unchanged by the selected strategy.
    assert_eq!(view.exposure_benchmark.role, "primary");
    assert_eq!(view.exposure_benchmark.method, "exposure_matched");
}

#[tokio::test]
async fn the_dashboard_human_and_json_sections_carry_the_same_strategy_semantics() {
    let (db, sources) = dashboard_fixture().await;
    let today = clock();
    let blend_id = save_strategy_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let view = portfolio_view(&db, &sources, today).await;
    let human = view_human(&view);
    let json = view_json(&view);

    let section = section_after(&human, "Strategy benchmark:");
    for expected in [
        "Strategy comparison: available",
        "Comparison window: 02-01-2025 → 04-02-2025",
        "Strategy anchor: 02-01-2025",
        "Selected at: 05-02-2025",
        "basis: explicit_user_selection",
        "Selected definition: Global Sixty Forty",
        "kind custom_blend",
        "Reset rule: monthly",
        "Global Sixty Forty",
        "target weight: 60.00%",
        "target weight: 40.00%",
        "Portfolio return:",
        "Excess:",
        "Since Comparison Start",
    ] {
        assert!(
            section.contains(expected),
            "the strategy section omitted {expected}: {section}"
        );
    }
    // The strategy section is labelled separately from the exposure section.
    let exposure_section = section_after(&human, "Exposure-matched benchmark:");
    assert!(exposure_section.contains("Primary exposure comparison"));
    assert!(
        !exposure_section.contains("Global Sixty Forty"),
        "the exposure section must not carry the strategy: {exposure_section}"
    );

    let strategy_json = &json["data"]["strategy_benchmark"];
    assert_eq!(strategy_json["role"], "strategy");
    assert_eq!(strategy_json["method"], "explicit_strategy_selection");
    assert_eq!(strategy_json["availability"]["status"], "available");
    assert_eq!(strategy_json["start_date"], WINDOW_START);
    assert_eq!(strategy_json["end_date"], WINDOW_END);
    assert_eq!(strategy_json["strategy_anchor_date"], WINDOW_START);
    assert_eq!(strategy_json["reset_rule"], "monthly");
    assert_eq!(strategy_json["reference_label"], "Global Sixty Forty");
    assert_close(
        strategy_json["subject_return_pct"]
            .as_f64()
            .expect("a portfolio return"),
        23.0,
    );
    assert_close(
        strategy_json["reference_return_pct"]
            .as_f64()
            .expect("a strategy return"),
        6.0,
    );
    assert_close(
        strategy_json["excess_return_percentage_points"]
            .as_f64()
            .expect("an excess return"),
        17.0,
    );
    assert_eq!(strategy_json["selection_basis"], "explicit_user_selection");
    assert_eq!(
        strategy_json["reference_id"],
        "custom-blend-global-sixty-forty"
    );
    assert_eq!(strategy_json["definition_kind"], "custom_blend");
    assert_eq!(strategy_json["verification_state"], "verified");
    assert_eq!(strategy_json["selected_at"], "2025-02-05T00:00:00Z");
    let components = strategy_json["components"]
        .as_array()
        .expect("the components");
    assert_eq!(components.len(), 2);
    assert_eq!(components[0]["target_weight_percent"], 60.0);
    assert_eq!(components[1]["is_cash"], true);
    let since_start = &strategy_json["standard_periods"][4];
    assert_eq!(since_start["period"], "since_comparison_start");
    assert_eq!(since_start["start_date"], WINDOW_START);
    assert_close(
        since_start["subject_return_pct"]["value"]
            .as_f64()
            .expect("a portfolio return"),
        23.0,
    );
    assert_close(
        since_start["reference_return_pct"]["value"]
            .as_f64()
            .expect("a strategy return"),
        6.0,
    );
    assert_close(
        since_start["excess_return_percentage_points"]["value"]
            .as_f64()
            .expect("an excess return"),
        17.0,
    );
    // The exposure comparison keeps its own role in the same envelope.
    assert_eq!(json["data"]["exposure_benchmark"]["role"], "primary");
    assert_eq!(
        json["data"]["exposure_benchmark"]["method"],
        "exposure_matched"
    );
}

#[tokio::test]
async fn both_dashboard_aliases_render_the_selected_strategy() {
    let (db, sources) = dashboard_fixture().await;
    let today = clock();
    let blend_id = save_strategy_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    for args in [&["rstock", "get"][..], &["rstock", "portfolio", "get"][..]] {
        let output = run_dashboard(&db, &sources, today, args, OutputFormat::Json).await;
        let json = common::parse_json_envelope(&output);
        assert_eq!(json["command"], "portfolio.get");
        assert_eq!(
            json["data"]["strategy_benchmark"]["role"], "strategy",
            "the alias {args:?} must show the selected strategy"
        );
        assert_eq!(
            json["data"]["strategy_benchmark"]["availability"]["status"],
            "available"
        );
    }
}

#[tokio::test]
async fn an_unavailable_strategy_keeps_the_exposure_comparison_and_the_portfolio_intact() {
    let (db, mut sources) = dashboard_fixture().await;
    let today = clock();
    let blend_id = save_strategy_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    // The strategy component's cached history disappears and its source
    // fails, so the selected Strategy benchmark cannot be prepared.
    benchmark_reference_price::Entity::delete_many()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(STRATEGY_REFERENCE))
        .exec(&db)
        .await
        .expect("the cached prices should be removable");
    sources.failing_price_sources.insert("XSTRAT".to_owned());
    let view = portfolio_view(&db, &sources, today).await;
    assert!(
        matches!(
            view.strategy_benchmark.availability,
            ComparisonAvailability::Unavailable { .. }
        ),
        "the strategy must be unavailable, got {:?}",
        view.strategy_benchmark.availability
    );
    // The exposure comparison and the portfolio-only outcomes stay intact.
    assert_eq!(
        view.exposure_benchmark.availability,
        ComparisonAvailability::Available
    );
    assert!(matches!(
        view.performance,
        rstock::models::PortfolioPerformance::Available(_)
    ));
    assert!(!view.nav_history.is_empty());

    let human = view_human(&view);
    let section = section_after(&human, "Strategy benchmark:");
    assert!(section.contains("Strategy comparison: unavailable"));
    // The selected definition and its provenance stay visible, so the reason
    // is inspectable instead of hidden behind a summary.
    assert!(section.contains("Selected definition: Global Sixty Forty"));
    assert!(section.contains("Reset rule: monthly"));
    assert!(section.contains("target weight: 60.00%"));
    let exposure_section = section_after(&human, "Exposure-matched benchmark:");
    assert!(exposure_section.contains("Primary exposure comparison: available"));
}

#[tokio::test]
async fn the_selected_strategy_never_changes_the_portfolio_accounting_or_valuation_date() {
    let (db, sources) = dashboard_fixture().await;
    let today = clock();
    let before = portfolio_view(&db, &sources, today).await;
    let blend_id = save_strategy_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");
    let after = portfolio_view(&db, &sources, today).await;

    let rstock::models::PortfolioPerformance::Available(before_performance) = &before.performance
    else {
        panic!("the portfolio performance should be available");
    };
    let rstock::models::PortfolioPerformance::Available(after_performance) = &after.performance
    else {
        panic!("the portfolio performance should be available");
    };
    assert_eq!(
        before_performance.effective_valuation_date, after_performance.effective_valuation_date,
        "the Effective valuation date must not change"
    );
    assert_eq!(before_performance.nav, after_performance.nav);
    assert_eq!(
        before_performance.synchronized_value,
        after_performance.synchronized_value
    );
    let history_before: Vec<(String, f64, f64)> = before
        .nav_history
        .iter()
        .map(|snapshot| (snapshot.date.clone(), snapshot.total_value, snapshot.nav))
        .collect();
    let history_after: Vec<(String, f64, f64)> = after
        .nav_history
        .iter()
        .map(|snapshot| (snapshot.date.clone(), snapshot.total_value, snapshot.nav))
        .collect();
    assert_eq!(history_before, history_after);
    assert_eq!(
        before.inventory.performance_holdings.positions.len(),
        after.inventory.performance_holdings.positions.len(),
        "the inventory must not change"
    );
}

#[tokio::test]
async fn the_output_adapters_never_fetch_market_data() {
    let (db, sources) = dashboard_fixture().await;
    let today = clock();
    let blend_id = save_strategy_blend(&db, &sources, today).await;
    let market_data = market_data_at(&sources, today);
    strategy::select_strategy(&db, &market_data, &blend_id)
        .await
        .expect("the strategy should be selected");

    let view = portfolio_view(&db, &sources, today).await;
    let calls_before = sources.historical_source_call_count();
    let _ = view_human(&view);
    let _ = view_json(&view);
    assert_eq!(
        sources.historical_source_call_count(),
        calls_before,
        "the output Adapters must not request any market data"
    );
}
