#![allow(clippy::float_cmp)]

//! Public fund-comparison behavior for ticket #119: every compared fund is
//! measured against its own selected primary reference, and the comparison
//! never implies that two different references describe one investment
//! universe.

pub mod common;

use chrono::Datelike;
use common::{insert_fund_asset, market_data_at, setup_test_db, MockMarketDataSources};
use rstock::cli::CorrelationPeriod;
use rstock::constants::format_date;
use rstock::db::entities::{benchmark_assignment, benchmark_reference, transaction};
use rstock::models::{FundComparisonPeriod, FundData, FundQuoteMetadata};
use rstock::services::fund_comparison::compare_funds;
use rstock::services::market_data::MarketData;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde_json::Value;

const TODAY: chrono::NaiveDate = chrono::NaiveDate::from_ymd_opt(2025, 1, 9).unwrap();

/// The JSON envelope of the production fund-comparison command.
///
/// The envelope comes from the library dispatch of a real command value, so
/// the command identity and the document shape are the production ones.
async fn fund_comparison_envelope(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code_a: &str,
    code_b: &str,
    period: CorrelationPeriod,
) -> Value {
    let mut output = Vec::new();
    rstock::cli::run_command_with_writer(
        db,
        market_data,
        rstock::cli::Commands::Compare(rstock::cli::CompareArgs {
            command: rstock::cli::CompareCommands::Funds {
                code_a: code_a.to_owned(),
                code_b: code_b.to_owned(),
                period,
            },
        }),
        rstock::cli::output::OutputFormat::Json,
        &mut output,
    )
    .await
    .expect("the fund comparison command should succeed");
    let text = String::from_utf8(output).expect("the command output should be UTF-8");
    assert_eq!(text.lines().count(), 1);
    assert!(!text.contains("\u{1b}["));
    serde_json::from_str(&text).expect("the command output should be valid JSON")
}

fn human_context(result: &rstock::models::FundComparisonResult) -> String {
    let mut human = Vec::new();
    rstock::cli::display::write_fund_comparison_reference_context(&mut human, result)
        .expect("fund comparison reference context should serialize");
    String::from_utf8(human).expect("fund comparison reference context should be UTF-8")
}

fn human_identity(result: &rstock::models::FundComparisonResult) -> String {
    let mut human = Vec::new();
    rstock::cli::display::write_fund_comparison_identity(&mut human, result)
        .expect("fund comparison identity should serialize");
    String::from_utf8(human).expect("fund comparison identity should be UTF-8")
}

fn qualification(
    currency: &str,
    basis: &str,
    adjustment: &str,
    hedge: &str,
    cadence: &str,
) -> rstock::services::market_data::SeriesQualification {
    rstock::services::market_data::SeriesQualification {
        currency: currency.to_owned(),
        return_basis: basis.to_owned(),
        adjustment: adjustment.to_owned(),
        hedge_policy: hedge.to_owned(),
        cadence: cadence.to_owned(),
    }
}

fn thirty_day_period() -> FundComparisonPeriod {
    FundComparisonPeriod {
        label: "30D",
        days: 30,
    }
}

fn one_year_period() -> FundComparisonPeriod {
    FundComparisonPeriod {
        label: "1Y",
        days: 365,
    }
}

/// Inserts one verified reference definition with its own purpose.
async fn insert_reference(
    db: &DatabaseConnection,
    id: &str,
    display_name: &str,
    universe: &str,
    instrument_ticker: &str,
    official_comparator: Option<&str>,
) {
    benchmark_reference::Entity::insert(benchmark_reference::ActiveModel {
        id: Set(id.to_owned()),
        display_name: Set(display_name.to_owned()),
        instrument_ticker: Set(instrument_ticker.to_owned()),
        source: Set("test_source".to_owned()),
        source_identifier: Set(instrument_ticker.to_owned()),
        currency: Set("EUR".to_owned()),
        adjustment_provenance: Set("explicit fake total-return observations".to_owned()),
        adjustment_qualification: Set("dividends_reinvested_once".to_owned()),
        return_basis_qualification: Set("total_return".to_owned()),
        role: Set("Investment universe".to_owned()),
        universe: Set(universe.to_owned()),
        hedge_policy: Set("Unhedged".to_owned()),
        source_provenance: Set("deterministic test source".to_owned()),
        history_boundary: Set("2025-01-06 exact fixture class".to_owned()),
        verification_state: Set("verified".to_owned()),
        official_comparator: Set(official_comparator.map(str::to_owned)),
        objective_comparator: Set(Some("Stated objective comparator".to_owned())),
        ..Default::default()
    })
    .exec(db)
    .await
    .expect("failed to insert benchmark reference");
}

/// Assigns one primary reference to one tracked fund.
async fn assign_reference(db: &DatabaseConnection, isin: &str, reference_id: &str) {
    benchmark_assignment::Entity::insert(benchmark_assignment::ActiveModel {
        subject_type: Set("tracked".to_owned()),
        subject_id: Set(isin.to_owned()),
        reference_id: Set(reference_id.to_owned()),
        is_override: Set(false),
        assigned_at: Set("2025-01-01T00:00:00Z".to_owned()),
    })
    .exec(db)
    .await
    .expect("failed to insert benchmark assignment");
}

/// One compared fund: its tracked class, its source series, and its
/// reference series.
struct FundFixture {
    isin: &'static str,
    name: &'static str,
    code: &'static str,
    reference_identifier: &'static str,
}

/// Registers one fund with a weekday price history that covers the selected
/// comparison period.
fn register_fund(sources: &mut MockMarketDataSources, fund: &FundFixture) {
    sources.fund_data.insert(
        fund.code.to_owned(),
        FundData {
            fund_currency: Some("EUR".to_owned()),
            total_holdings: Some(10),
            portfolio_date: Some("2025-01-06".to_owned()),
            holdings: vec![],
        },
    );
    sources.fund_quote_metadata.insert(
        fund.code.to_owned(),
        FundQuoteMetadata {
            quote_currency: Some("EUR".to_owned()),
            ..Default::default()
        },
    );
    sources
        .historical_prices
        .insert(fund.code.to_owned(), weekday_series(100.0, 0.01));
    sources.historical_prices.insert(
        fund.reference_identifier.to_owned(),
        weekday_series(50.0, 0.005),
    );
    sources.series_qualifications.insert(
        ("morningstar".to_owned(), fund.code.to_owned()),
        qualification(
            "EUR",
            "total_return",
            "dividends_reinvested_once",
            "Unhedged",
            "daily",
        ),
    );
    sources.series_qualifications.insert(
        (
            "test_source".to_owned(),
            fund.reference_identifier.to_owned(),
        ),
        qualification(
            "EUR",
            "total_return",
            "dividends_reinvested_once",
            "Unhedged",
            "daily",
        ),
    );
}

/// The completed weekday closes of the comparison window.
fn weekday_series(base: f64, step: f64) -> Vec<(String, f64)> {
    let mut date = chrono::NaiveDate::from_ymd_opt(2024, 11, 1).unwrap();
    let end = chrono::NaiveDate::from_ymd_opt(2025, 1, 8).unwrap();
    let mut series = Vec::new();
    while date <= end {
        if !matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
            series.push((format_date(date), base + step * series.len() as f64));
        }
        date += chrono::Duration::days(1);
    }
    series
}

/// Two compared funds with two different assigned references.
async fn unlike_references_fixture() -> (DatabaseConnection, MockMarketDataSources) {
    let db = setup_test_db().await;
    let alpha = FundFixture {
        isin: "XSIEIN0001",
        name: "Cobas Internacional FI D",
        code: "XCLASS1",
        reference_identifier: "XREF1",
    };
    let beta = FundFixture {
        isin: "XSIEIN0002",
        name: "Cobas Internacional FI A",
        code: "XCLASS2",
        reference_identifier: "XREF2",
    };
    insert_fund_asset(&db, alpha.isin, alpha.name, "EUR", alpha.code).await;
    insert_fund_asset(&db, beta.isin, beta.name, "EUR", beta.code).await;
    insert_reference(
        &db,
        "reference-alpha",
        "MSCI World",
        "Developed-market large/mid-cap equities",
        alpha.reference_identifier,
        Some("MSCI World Index"),
    )
    .await;
    insert_reference(
        &db,
        "reference-beta",
        "MSCI ACWI Small Cap Value",
        "Global small-cap value equities",
        beta.reference_identifier,
        Some("MSCI ACWI Small Cap Value Index"),
    )
    .await;
    assign_reference(&db, alpha.isin, "reference-alpha").await;
    assign_reference(&db, beta.isin, "reference-beta").await;

    let mut sources = MockMarketDataSources::new();
    register_fund(&mut sources, &alpha);
    register_fund(&mut sources, &beta);
    (db, sources)
}

#[tokio::test]
async fn each_fund_is_measured_against_its_own_reference() {
    let (db, sources) = unlike_references_fixture().await;
    let market = market_data_at(&sources, TODAY);
    let result = compare_funds(&db, &market, "XCLASS1", "XCLASS2", thirty_day_period())
        .await
        .expect("fund comparison should succeed");

    let data = fund_comparison_envelope(
        &db,
        &market,
        "XCLASS1",
        "XCLASS2",
        CorrelationPeriod::ThirtyDays,
    )
    .await["data"]
        .clone();

    // Each side keeps its own reference identity, purpose, and conventions.
    let alpha = &data["fund_a"]["primary_benchmark_comparison"];
    let beta = &data["fund_b"]["primary_benchmark_comparison"];
    assert_eq!(alpha["reference_id"], "reference-alpha");
    assert_eq!(alpha["reference_name"], "MSCI World");
    assert_eq!(alpha["source"], "test_source");
    assert_eq!(alpha["source_identifier"], "XREF1");
    assert_eq!(alpha["currency"], "EUR");
    assert_eq!(alpha["native_currency"], "EUR");
    assert_eq!(alpha["return_basis"], "total_return");
    assert_eq!(alpha["adjustment"], "dividends_reinvested_once");
    assert_eq!(alpha["hedge_policy"], "Unhedged");
    assert_eq!(alpha["purpose"]["role"], "Investment universe");
    assert_eq!(
        alpha["purpose"]["universe"],
        "Developed-market large/mid-cap equities"
    );
    assert_eq!(alpha["purpose"]["official_comparator"], "MSCI World Index");
    assert_eq!(
        alpha["purpose"]["objective_comparator"],
        "Stated objective comparator"
    );
    assert_eq!(
        alpha["purpose"]["history_boundary"],
        "2025-01-06 exact fixture class"
    );
    assert_eq!(alpha["purpose"]["verification_state"], "verified");
    assert_eq!(alpha["purpose"]["instrument_ticker"], "XREF1");

    assert_eq!(beta["reference_id"], "reference-beta");
    assert_eq!(beta["reference_name"], "MSCI ACWI Small Cap Value");
    assert_eq!(
        beta["purpose"]["universe"],
        "Global small-cap value equities"
    );
    assert_eq!(
        beta["purpose"]["official_comparator"],
        "MSCI ACWI Small Cap Value Index"
    );

    // The two sides are not measured against one shared reference.
    assert_ne!(alpha["reference_id"], beta["reference_id"]);
    assert_eq!(alpha["availability"]["status"], "available");
    assert_eq!(beta["availability"]["status"], "available");
    // Each side reports the actual comparison dates of its own comparison.
    assert_eq!(alpha["start_date"], "2024-11-01");
    assert_eq!(alpha["end_date"], "2025-01-08");
    assert_eq!(beta["start_date"], "2024-11-01");
    assert_eq!(beta["end_date"], "2025-01-08");

    // The comparison carries no shared legacy reference any more.
    assert!(data.get("benchmark_reference").is_none());
    assert!(data["fund_a"].get("benchmark_limitations").is_none());

    let human = human_context(&result);
    assert!(human.contains("MSCI World"));
    assert!(human.contains("MSCI ACWI Small Cap Value"));
    assert!(human.contains("Reference purpose: role Investment universe"));
    assert!(!human.contains("legacy-acwi-etf-proxy"));

    // The human output carries the same reference identity, purpose,
    // conventions, dates, and scopes as the JSON output.
    assert!(
        human.contains("Cobas Internacional FI D primary reference: MSCI World [reference-alpha]")
    );
    assert!(human.contains(
        "Cobas Internacional FI A primary reference: MSCI ACWI Small Cap Value [reference-beta]"
    ));
    assert!(human.contains("native currency EUR"));
    assert!(human.contains("basis total_return"));
    assert!(human.contains("hedge Unhedged"));
    assert!(human.contains("universe Developed-market large/mid-cap equities"));
    assert!(human.contains("universe Global small-cap value equities"));
    assert!(human.contains("Official comparator: MSCI World Index"));
    assert!(human.contains("Comparison dates: 01-11-2024 – 08-01-2025"));
    assert!(human.contains("Since Comparison Start"));
    assert!(human.contains("Aligned wealth graph (EUR wealth rebased to 100)"));

    // The exact class of each side stays visible in the human output.
    let identity = human_identity(&result);
    assert!(identity.contains("Exact class: XSIEIN0001 (fund, tracked in this portfolio)"));
    assert!(identity.contains("Exact class: XSIEIN0002 (fund, tracked in this portfolio)"));
}

#[tokio::test]
async fn an_unusable_reference_affects_only_its_own_side() {
    let (db, sources) = unlike_references_fixture().await;
    // The second fund keeps its tracked class but loses its assignment.
    benchmark_assignment::Entity::delete_many()
        .filter(benchmark_assignment::Column::SubjectId.eq("XSIEIN0002"))
        .exec(&db)
        .await
        .expect("failed to remove the second assignment");

    let market = market_data_at(&sources, TODAY);
    let data = fund_comparison_envelope(
        &db,
        &market,
        "XCLASS1",
        "XCLASS2",
        CorrelationPeriod::ThirtyDays,
    )
    .await["data"]
        .clone();
    assert_eq!(
        data["fund_a"]["primary_benchmark_comparison"]["availability"]["status"],
        "available"
    );
    assert_eq!(
        data["fund_b"]["primary_benchmark_comparison"]["availability"]["status"],
        "unavailable"
    );
    assert!(
        data["fund_b"]["primary_benchmark_comparison"]["availability"]["reason"]
            .as_str()
            .unwrap()
            .contains("no explicit primary reference")
    );
    // The failing side never borrows the other side's reference.
    assert!(data["fund_b"]["primary_benchmark_comparison"]["purpose"].is_null());
    assert!(data["fund_b"]["primary_benchmark_comparison"]["reference_id"].is_null());
    assert_eq!(
        data["fund_a"]["primary_benchmark_comparison"]["reference_id"],
        "reference-alpha"
    );

    // The direct fund-to-fund analysis and the fund-only rows stay available.
    assert!(data["correlation"]["correlation"].is_number());
    assert!(data["fund_b"]["all_time"]["total_return"].is_number());
    assert!(data["fund_b"]["all_time"]["sharpe"].is_number());
}

#[tokio::test]
async fn exact_classes_and_vehicles_stay_distinct_and_create_no_holding() {
    let (db, sources) = unlike_references_fixture().await;
    let assets_before = rstock::db::entities::asset::Entity::find()
        .count(&db)
        .await
        .expect("failed to count assets");
    let transactions_before = transaction::Entity::find()
        .count(&db)
        .await
        .expect("failed to count transactions");

    let market = market_data_at(&sources, TODAY);
    let data = fund_comparison_envelope(
        &db,
        &market,
        "XCLASS1",
        "XCLASS2",
        CorrelationPeriod::ThirtyDays,
    )
    .await["data"]
        .clone();
    assert_eq!(
        data["fund_a"]["class_identity"]["tracked_identity"],
        "XSIEIN0001"
    );
    assert_eq!(
        data["fund_b"]["class_identity"]["tracked_identity"],
        "XSIEIN0002"
    );
    assert_eq!(data["fund_a"]["class_identity"]["vehicle"], "fund");
    assert_ne!(
        data["fund_a"]["class_identity"]["tracked_identity"],
        data["fund_b"]["class_identity"]["tracked_identity"]
    );
    assert_ne!(data["fund_a"]["name"], data["fund_b"]["name"]);

    // The comparison never creates a holding.
    assert_eq!(
        rstock::db::entities::asset::Entity::find()
            .count(&db)
            .await
            .expect("failed to count assets"),
        assets_before
    );
    assert_eq!(
        transaction::Entity::find()
            .count(&db)
            .await
            .expect("failed to count transactions"),
        transactions_before
    );
}

#[tokio::test]
async fn an_untracked_candidate_keeps_its_absence_visible() {
    let (db, mut sources) = unlike_references_fixture().await;
    // A third fund is analysed without being tracked: it has no local class
    // identity and no assignment.
    let candidate = FundFixture {
        isin: "XSIEIN0003",
        name: "Untracked Candidate Fund",
        code: "XCANDIDATE",
        reference_identifier: "XREFCANDIDATE",
    };
    register_fund(&mut sources, &candidate);

    let market = market_data_at(&sources, TODAY);
    let result = compare_funds(&db, &market, "XCANDIDATE", "XCLASS1", thirty_day_period())
        .await
        .expect("fund comparison should succeed");

    let data = fund_comparison_envelope(
        &db,
        &market,
        "XCANDIDATE",
        "XCLASS1",
        CorrelationPeriod::ThirtyDays,
    )
    .await["data"]
        .clone();
    assert!(data["fund_a"]["class_identity"]["tracked_identity"].is_null());
    assert!(data["fund_a"]["class_identity"]["vehicle"].is_null());
    assert_eq!(
        data["fund_a"]["class_identity"]["source_code"],
        "XCANDIDATE"
    );
    let identity = human_identity(&result);
    assert!(identity.contains(
        "Exact class: XCANDIDATE is not tracked in this portfolio; only the source code identifies this exact class"
    ));
    assert_eq!(
        data["fund_a"]["primary_benchmark_comparison"]["availability"]["status"],
        "unavailable"
    );
    assert!(
        data["fund_a"]["primary_benchmark_comparison"]["availability"]["reason"]
            .as_str()
            .unwrap()
            .contains("no exact tracked-asset identity")
    );
    // The other side keeps its own reference.
    assert_eq!(
        data["fund_b"]["primary_benchmark_comparison"]["reference_id"],
        "reference-alpha"
    );
}

#[tokio::test]
async fn the_graph_period_does_not_redefine_the_standard_rows() {
    let (db, sources) = unlike_references_fixture().await;
    let market = market_data_at(&sources, TODAY);
    let short_graph = compare_funds(&db, &market, "XCLASS1", "XCLASS2", thirty_day_period())
        .await
        .expect("fund comparison should succeed");
    let long_graph = compare_funds(&db, &market, "XCLASS1", "XCLASS2", one_year_period())
        .await
        .expect("fund comparison should succeed");

    assert_eq!(
        short_graph
            .fund_a
            .primary_benchmark_comparison
            .standard_periods,
        long_graph
            .fund_a
            .primary_benchmark_comparison
            .standard_periods
    );
    assert_eq!(
        short_graph
            .fund_b
            .primary_benchmark_comparison
            .standard_periods,
        long_graph
            .fund_b
            .primary_benchmark_comparison
            .standard_periods
    );
    assert_eq!(short_graph.correlation.period_label, "30D");
    assert_eq!(long_graph.correlation.period_label, "1Y");
}

#[tokio::test]
async fn the_direct_comparison_keeps_its_full_period_coverage_rule() {
    let (db, mut sources) = unlike_references_fixture().await;
    // The second fund loses the selected period's start coverage.
    sources.historical_prices.insert(
        "XCLASS2".to_owned(),
        vec![
            ("2025-01-08".to_owned(), 106.0),
            ("2025-01-09".to_owned(), 107.0),
        ],
    );

    let market = market_data_at(&sources, TODAY);
    let data = fund_comparison_envelope(
        &db,
        &market,
        "XCLASS1",
        "XCLASS2",
        CorrelationPeriod::ThirtyDays,
    )
    .await["data"]
        .clone();
    assert!(data["correlation"]["correlation"].is_null());
    assert!(data["correlation"]["points"].as_array().unwrap().is_empty());
    assert!(data["correlation"]["reason"]
        .as_str()
        .unwrap()
        .contains("lacks selected-period start coverage"));
    // The reference-relative rows of the covered side stay available.
    assert_eq!(
        data["fund_a"]["primary_benchmark_comparison"]["availability"]["status"],
        "available"
    );
}

#[tokio::test]
async fn a_scoped_limitation_stays_in_its_own_scope() {
    let (db, mut sources) = unlike_references_fixture().await;
    // The first fund's reference stops before the comparison end, so the
    // reference scope carries the limitation while the fund scope stays clear.
    let mut reference_prices = sources
        .historical_prices
        .get("XREF1")
        .cloned()
        .expect("the first reference should have prices");
    reference_prices.retain(|(date, _)| date != "2025-01-08");
    sources
        .historical_prices
        .insert("XREF1".to_owned(), reference_prices);

    let market = market_data_at(&sources, TODAY);
    let result = compare_funds(&db, &market, "XCLASS1", "XCLASS2", thirty_day_period())
        .await
        .expect("fund comparison should succeed");

    let data = fund_comparison_envelope(
        &db,
        &market,
        "XCLASS1",
        "XCLASS2",
        CorrelationPeriod::ThirtyDays,
    )
    .await["data"]
        .clone();
    let alpha = &data["fund_a"]["primary_benchmark_comparison"];
    assert_eq!(alpha["availability"]["status"], "unavailable");
    assert!(alpha["fund_limitations"].as_array().unwrap().is_empty());
    assert!(!alpha["reference_limitations"]
        .as_array()
        .unwrap()
        .is_empty());

    // The other side keeps its own complete comparison.
    assert_eq!(
        data["fund_b"]["primary_benchmark_comparison"]["availability"]["status"],
        "available"
    );

    let human = human_context(&result);
    assert!(human.contains("Reference limitation: Market data limitation: benchmark reference"));
    assert!(!human.contains("Fund limitation: Market data limitation: benchmark reference"));
}

#[tokio::test]
async fn the_compare_funds_command_dispatches_one_json_envelope() {
    let (db, sources) = unlike_references_fixture().await;
    // The second fund keeps its class but loses its assignment, so the
    // dispatched document shows one usable reference and one isolated failure.
    benchmark_assignment::Entity::delete_many()
        .filter(benchmark_assignment::Column::SubjectId.eq("XSIEIN0002"))
        .exec(&db)
        .await
        .expect("failed to remove the second assignment");
    let market = market_data_at(&sources, TODAY);

    let mut output = Vec::new();
    rstock::cli::run_command_with_writer(
        &db,
        &market,
        rstock::cli::Commands::Compare(rstock::cli::CompareArgs {
            command: rstock::cli::CompareCommands::Funds {
                code_a: "XCLASS1".to_owned(),
                code_b: "XCLASS2".to_owned(),
                period: CorrelationPeriod::ThirtyDays,
            },
        }),
        rstock::cli::output::OutputFormat::Json,
        &mut output,
    )
    .await
    .expect("the fund comparison command should succeed");

    // The successful output of the command is exactly one envelope: one line,
    // one JSON value, no human text and no ANSI sequences beside it.
    let text = String::from_utf8(output).expect("the command output should be UTF-8");
    assert_eq!(text.lines().count(), 1);
    assert!(text.ends_with('\n'));
    assert!(!text.contains("\u{1b}["));
    let envelope: Value = serde_json::from_str(&text).expect("valid JSON envelope");
    assert_eq!(envelope["command"], "compare.funds");
    assert!(envelope["data"].is_object());
    assert!(envelope.get("error").is_none());

    // The dispatched document keeps the per-side reference facts and the
    // isolated failure of the side without a reference.
    let data = &envelope["data"];
    assert_eq!(
        data["fund_a"]["primary_benchmark_comparison"]["reference_id"],
        "reference-alpha"
    );
    assert_eq!(
        data["fund_a"]["primary_benchmark_comparison"]["reference_name"],
        "MSCI World"
    );
    assert_eq!(
        data["fund_b"]["primary_benchmark_comparison"]["availability"]["status"],
        "unavailable"
    );
    assert!(data["fund_b"]["primary_benchmark_comparison"]["reference_id"].is_null());
    assert!(data["fund_b"]["primary_benchmark_comparison"]["purpose"].is_null());
    assert!(data["correlation"]["correlation"].is_number());

    // The human output carries the same facts through the production human
    // writers. The human renderer itself keeps printing to the process
    // stdout, because the chart library has no writer API, so this test does
    // not claim a captured human stdout.
    let result = compare_funds(&db, &market, "XCLASS1", "XCLASS2", thirty_day_period())
        .await
        .expect("fund comparison should succeed");
    let identity = human_identity(&result);
    let context = human_context(&result);
    assert!(identity.contains("Exact class: XSIEIN0001 (fund, tracked in this portfolio)"));
    assert!(identity.contains("Exact class: XSIEIN0002 (fund, tracked in this portfolio)"));
    let reference_name = data["fund_a"]["primary_benchmark_comparison"]["reference_name"]
        .as_str()
        .expect("the dispatched reference should have a name");
    assert!(context.contains(reference_name));
    assert!(context.contains("assigned primary comparison: unavailable"));
}

#[tokio::test]
async fn the_shared_preparation_is_reused_for_both_paths() {
    let (db, sources) = unlike_references_fixture().await;
    let market = market_data_at(&sources, TODAY);
    compare_funds(&db, &market, "XCLASS1", "XCLASS2", thirty_day_period())
        .await
        .expect("fund comparison should succeed");

    let ranges = sources.historical_price_request_ranges();
    let fund_requests = ranges
        .iter()
        .filter(|(identity, _, _)| identity == "XCLASS1" || identity == "XCLASS2")
        .count();
    assert_eq!(
        fund_requests, 2,
        "each compared fund is prepared once for the direct and the reference comparison"
    );
    let reference_requests = sources
        .benchmark_price_source_requests
        .lock()
        .expect("benchmark price request mutex poisoned")
        .clone();
    assert_eq!(
        reference_requests
            .iter()
            .filter(|(source, identifier)| {
                source == "test_source" && (identifier == "XREF1" || identifier == "XREF2")
            })
            .count(),
        2,
        "each reference is prepared once, not once per date"
    );
}

#[tokio::test]
async fn a_native_currency_fund_is_never_compared_with_an_unconverted_reference() {
    let (db, mut sources) = unlike_references_fixture().await;
    // The first fund is a USD class while its reference is a EUR definition.
    sources.fund_data.get_mut("XCLASS1").unwrap().fund_currency = Some("USD".to_owned());
    sources
        .fund_quote_metadata
        .get_mut("XCLASS1")
        .unwrap()
        .quote_currency = Some("USD".to_owned());
    sources.series_qualifications.insert(
        ("morningstar".to_owned(), "XCLASS1".to_owned()),
        qualification(
            "USD",
            "total_return",
            "dividends_reinvested_once",
            "Unhedged",
            "daily",
        ),
    );

    let market = market_data_at(&sources, TODAY);
    let data = fund_comparison_envelope(
        &db,
        &market,
        "XCLASS1",
        "XCLASS2",
        CorrelationPeriod::ThirtyDays,
    )
    .await["data"]
        .clone();
    let comparison = &data["fund_a"]["primary_benchmark_comparison"];
    assert_eq!(comparison["availability"]["status"], "unavailable");
    assert!(comparison["fund_wealth"].as_array().unwrap().is_empty());
    assert!(comparison["reference_wealth"]
        .as_array()
        .unwrap()
        .is_empty());
    // The other side keeps its own complete EUR comparison.
    assert_eq!(
        data["fund_b"]["primary_benchmark_comparison"]["availability"]["status"],
        "available"
    );
    assert_eq!(
        data["fund_b"]["primary_benchmark_comparison"]["currency"],
        "EUR"
    );
}

#[tokio::test]
async fn the_holdings_snapshot_behavior_is_preserved() {
    let (db, sources) = unlike_references_fixture().await;
    let market = market_data_at(&sources, TODAY);
    compare_funds(&db, &market, "XCLASS1", "XCLASS2", thirty_day_period())
        .await
        .expect("fund comparison should succeed");

    for code in ["XCLASS1", "XCLASS2"] {
        let snapshot = rstock::db::repos::fund_holdings_snapshot_repo::find_latest(&db, code)
            .await
            .expect("failed to read the holdings snapshot")
            .expect("the compared fund should keep a holdings snapshot");
        assert_eq!(snapshot.snapshot_date, "2025-01-06");
    }
}
