//! Public behaviour of ticket #120: optional secondary comparisons.
//!
//! The feature is exercised offline with an in-memory database, dummy
//! identities, fixed clocks, and fake `MarketDataSources`. Every expectation
//! is derived from the persisted assignments, the fake observations, and the
//! shared Benchmark Module, so a change in the separation between the primary
//! and the secondary comparison fails the test instead of silently changing
//! the reported comparison.

#![allow(clippy::float_cmp)]

pub mod common;

use chrono::{Datelike, NaiveDate};
use clap::Parser;
use common::{
    insert_asset, insert_fund_asset, insert_portfolio_asset_snapshot, insert_portfolio_snapshot,
    insert_transaction, market_data_at, setup_test_db, MockMarketDataSources,
};
use rstock::cli::output::OutputFormat;
use rstock::constants::format_date;
use rstock::db::entities::{
    benchmark_assignment, benchmark_reference, benchmark_reference_price, portfolio_asset_history,
    portfolio_history, transaction,
};
use rstock::models::{
    CandidateCorrelationPeriod, ComparisonFact, FundData, FundQuoteMetadata, PortfolioSnapshot,
};
use rstock::services::benchmark::compute_standard_benchmark_periods;
use rstock::services::exposure_benchmark::compute_exposure_matched_comparison;
use rstock::services::fund_analysis::compute_fund_analysis;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde_json::Value;

const TODAY: NaiveDate = NaiveDate::from_ymd_opt(2025, 1, 9).expect("fixed clock date");
const FUND_ISIN: &str = "XSIEIN000001";
const FUND_CODE: &str = "XCLASS";
const FUND_ISIN_B: &str = "XSIEIN000002";
const FUND_CODE_B: &str = "XCLASSB";
const PRIMARY_REFERENCE: &str = "verified-primary-reference";
const PRIMARY_REFERENCE_B: &str = "verified-primary-reference-b";
const SECONDARY_REFERENCE: &str = "verified-secondary-reference";
const REPLACEMENT_REFERENCE: &str = "verified-replacement-reference";
const PRIMARY_IDENTIFIER: &str = "XREFPRI";
const PRIMARY_IDENTIFIER_B: &str = "XREFPRB";
const SECONDARY_IDENTIFIER: &str = "XREFSEC";
const REPLACEMENT_IDENTIFIER: &str = "XREFREP";
const CANDIDATE_ISIN: &str = "LU1598719752";

// --- Command helpers ---

fn benchmark_command(args: &[&str]) -> rstock::cli::BenchmarkCommands {
    match rstock::cli::Cli::try_parse_from(args)
        .expect("the command should parse")
        .command
    {
        rstock::cli::Commands::Benchmark(arguments) => arguments.command,
        other => panic!("expected a benchmark command, got {other:?}"),
    }
}

fn blend_command(args: &[&str]) -> rstock::cli::BlendCommands {
    match rstock::cli::Cli::try_parse_from(args)
        .expect("the command should parse")
        .command
    {
        rstock::cli::Commands::Blend(arguments) => arguments.command,
        other => panic!("expected a blend command, got {other:?}"),
    }
}

async fn run_benchmark(
    db: &DatabaseConnection,
    command: rstock::cli::BenchmarkCommands,
    format: OutputFormat,
) -> anyhow::Result<Vec<u8>> {
    let mut output = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(db, command, format, &mut output).await?;
    Ok(output)
}

async fn run_blend(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
    command: rstock::cli::BlendCommands,
) -> anyhow::Result<Vec<u8>> {
    let market_data = market_data_at(sources, TODAY);
    let mut output = Vec::new();
    rstock::cli::run_blend_command_with_writer(
        db,
        &market_data,
        command,
        OutputFormat::Json,
        &mut output,
    )
    .await?;
    Ok(output)
}

// --- Fixture helpers ---

/// One reference definition with a verified production mapping.
async fn insert_verified_reference(
    db: &DatabaseConnection,
    id: &str,
    name: &str,
    identifier: &str,
) {
    benchmark_reference::Entity::insert(benchmark_reference::ActiveModel {
        id: Set(id.to_owned()),
        display_name: Set(name.to_owned()),
        instrument_ticker: Set(identifier.to_owned()),
        source: Set("test_source".to_owned()),
        source_identifier: Set(identifier.to_owned()),
        currency: Set("EUR".to_owned()),
        adjustment_provenance: Set("explicit fake total-return observations".to_owned()),
        adjustment_qualification: Set("dividends_reinvested_once".to_owned()),
        return_basis_qualification: Set("total_return".to_owned()),
        role: Set("Investment universe".to_owned()),
        universe: Set("test universe".to_owned()),
        hedge_policy: Set("Unhedged".to_owned()),
        source_provenance: Set("deterministic test source".to_owned()),
        history_boundary: Set("2025-01-08 exact fixture class".to_owned()),
        verification_state: Set("verified".to_owned()),
        official_comparator: Set(Some("Stated official comparator".to_owned())),
        objective_comparator: Set(Some("Stated objective comparator".to_owned())),
        ..Default::default()
    })
    .exec(db)
    .await
    .expect("the reference definition should persist");
}

/// One primary assignment of a reference to a subject.
async fn assign_primary(db: &DatabaseConnection, subject_type: &str, subject_id: &str, id: &str) {
    benchmark_assignment::Entity::insert(benchmark_assignment::ActiveModel {
        subject_type: Set(subject_type.to_owned()),
        subject_id: Set(subject_id.to_owned()),
        reference_id: Set(id.to_owned()),
        is_override: Set(false),
        assigned_at: Set("2025-01-01T00:00:00Z".to_owned()),
    })
    .exec(db)
    .await
    .expect("the primary assignment should persist");
}

/// The completed weekday closes of the comparison window.
fn weekday_series(base: f64, step: f64) -> Vec<(String, f64)> {
    series_between("2024-11-01", "2025-01-08", base, step)
}

/// The completed weekday closes between two dates.
fn series_between(start: &str, end: &str, base: f64, step: f64) -> Vec<(String, f64)> {
    let mut date = NaiveDate::parse_from_str(start, "%Y-%m-%d").expect("valid start date");
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d").expect("valid end date");
    let mut series = Vec::new();
    while date <= end {
        if !matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
            series.push((format_date(date), base + step * series.len() as f64));
        }
        date += chrono::Duration::days(1);
    }
    series
}

fn daily_qualification() -> rstock::services::market_data::SeriesQualification {
    rstock::services::market_data::SeriesQualification {
        currency: "EUR".to_owned(),
        return_basis: "total_return".to_owned(),
        adjustment: "dividends_reinvested_once".to_owned(),
        hedge_policy: "Unhedged".to_owned(),
        cadence: "daily".to_owned(),
    }
}

/// One fund with a primary reference and two candidate secondary references.
///
/// The fund is a Tracked asset whose exact class identity is its ISIN, so
/// both the primary assignment and the secondary assignments describe the same
/// investment.
async fn fund_fixture() -> (DatabaseConnection, MockMarketDataSources) {
    let db = setup_test_db().await;
    insert_fund_asset(&db, FUND_ISIN, "Secondary Fixture Fund", "EUR", FUND_CODE).await;
    insert_fund_asset(
        &db,
        FUND_ISIN_B,
        "Secondary Fixture Fund B",
        "EUR",
        FUND_CODE_B,
    )
    .await;
    insert_verified_reference(
        &db,
        PRIMARY_REFERENCE,
        "Verified Primary Reference",
        PRIMARY_IDENTIFIER,
    )
    .await;
    insert_verified_reference(
        &db,
        PRIMARY_REFERENCE_B,
        "Verified Primary Reference B",
        PRIMARY_IDENTIFIER_B,
    )
    .await;
    insert_verified_reference(
        &db,
        SECONDARY_REFERENCE,
        "Verified Secondary Reference",
        SECONDARY_IDENTIFIER,
    )
    .await;
    insert_verified_reference(
        &db,
        REPLACEMENT_REFERENCE,
        "Verified Replacement Reference",
        REPLACEMENT_IDENTIFIER,
    )
    .await;
    assign_primary(&db, "tracked", FUND_ISIN, PRIMARY_REFERENCE).await;
    assign_primary(&db, "tracked", FUND_ISIN_B, PRIMARY_REFERENCE_B).await;

    let mut sources = MockMarketDataSources::new();
    for (code, base, step) in [(FUND_CODE, 100.0, 0.10), (FUND_CODE_B, 80.0, 0.08)] {
        sources.fund_data.insert(
            code.to_owned(),
            FundData {
                fund_currency: Some("EUR".to_owned()),
                total_holdings: Some(10),
                portfolio_date: Some("2025-01-08".to_owned()),
                holdings: vec![],
            },
        );
        sources.fund_quote_metadata.insert(
            code.to_owned(),
            FundQuoteMetadata {
                quote_currency: Some("EUR".to_owned()),
                ..Default::default()
            },
        );
        sources
            .historical_prices
            .insert(code.to_owned(), weekday_series(base, step));
        sources.series_qualifications.insert(
            ("morningstar".to_owned(), code.to_owned()),
            daily_qualification(),
        );
    }
    for (identifier, base, step) in [
        (PRIMARY_IDENTIFIER, 50.0, 0.05),
        (PRIMARY_IDENTIFIER_B, 40.0, 0.04),
        (SECONDARY_IDENTIFIER, 25.0, 0.02),
        (REPLACEMENT_IDENTIFIER, 20.0, 0.01),
    ] {
        sources
            .historical_prices
            .insert(identifier.to_owned(), weekday_series(base, step));
        sources.series_qualifications.insert(
            ("test_source".to_owned(), identifier.to_owned()),
            daily_qualification(),
        );
    }
    (db, sources)
}

/// The JSON envelope of one fund analysis.
fn analysis_envelope(result: &rstock::models::FundAnalysisResult) -> Value {
    let mut output = Vec::new();
    rstock::cli::output::write_json(&mut output, "analyze.fund", result)
        .expect("the fund analysis should serialize");
    common::parse_json_envelope(&output)["data"].clone()
}

/// The human reference context of one fund analysis.
fn analysis_human(result: &rstock::models::FundAnalysisResult) -> String {
    let mut output = Vec::new();
    rstock::cli::display::write_fund_analysis_reference_context(&mut output, result)
        .expect("the fund analysis human output should be written");
    String::from_utf8(output).expect("the human output should be UTF-8")
}

async fn analyze(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
) -> rstock::models::FundAnalysisResult {
    let market_data = market_data_at(sources, TODAY);
    compute_fund_analysis(
        db,
        &market_data,
        FUND_CODE,
        CandidateCorrelationPeriod {
            label: "30D",
            days: 30,
        },
    )
    .await
    .expect("the fund analysis should succeed")
}

// --- Assignment management ---

#[tokio::test]
async fn secondary_assignments_are_managed_through_the_public_commands() {
    let db = setup_test_db().await;
    insert_asset(&db, "XFAKE1", "Tracked fixture", "stock", "EUR").await;

    let output = run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            "XFAKE1",
            "msci-europe-small-cap-value",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary assignment should succeed");
    let assigned = common::parse_json_envelope(&output);
    assert_eq!(assigned["command"], "benchmark.assign_secondary");
    assert_eq!(assigned["data"]["subject_type"], "tracked");
    assert_eq!(assigned["data"]["subject_id"], "XFAKE1");
    assert_eq!(
        assigned["data"]["reference_id"],
        "msci-europe-small-cap-value"
    );
    assert_eq!(assigned["data"]["role"], "secondary");
    assert_eq!(assigned["data"]["retrospective"], true);
    assert_eq!(assigned["data"]["creates_holding"], false);
    assert_eq!(
        assigned["data"]["definition"]["display_name"],
        "MSCI Europe Small Cap Value"
    );
    assert_eq!(
        assigned["data"]["definition"]["verification_state"],
        "target_only"
    );

    // The catalog reports the secondary assignment beside the primary
    // assignments, never inside them.
    let list = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&["rstock", "benchmark", "list"]),
            OutputFormat::Json,
        )
        .await
        .expect("the catalog should render"),
    );
    assert_eq!(list["data"]["assignments"].as_array().unwrap().len(), 0);
    let secondaries = list["data"]["secondary_assignments"].as_array().unwrap();
    assert_eq!(secondaries.len(), 1);
    assert_eq!(
        secondaries[0]["reference_id"],
        "msci-europe-small-cap-value"
    );
    assert_eq!(secondaries[0]["role"], "secondary");
    assert_eq!(secondaries[0]["subject_id"], "XFAKE1");
    assert_eq!(secondaries[0]["retrospective"], true);
    assert_eq!(secondaries[0]["creates_holding"], false);
    assert!(list["data"]["secondary_assignment_semantics"]
        .as_str()
        .unwrap()
        .contains("never replace the primary"));
    let definition = list["data"]["definitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|definition| definition["id"] == "msci-europe-small-cap-value")
        .expect("the definition should be listed");
    assert_eq!(definition["secondary_assignment_count"], 1);
    assert_eq!(definition["assignment_count"], 0);

    let human = String::from_utf8(
        run_benchmark(
            &db,
            benchmark_command(&["rstock", "benchmark", "list"]),
            OutputFormat::Human,
        )
        .await
        .expect("the catalog should render"),
    )
    .expect("the human output should be UTF-8");
    assert!(human.contains("Explicit secondary assignments"));
    assert!(human.contains("MSCI Europe Small Cap Value"));
    assert!(human.contains("role: secondary"));

    // An explicit removal reports the removed identity.
    let removed = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&[
                "rstock",
                "benchmark",
                "remove-secondary",
                "--ticker",
                "XFAKE1",
                "--reference-id",
                "msci-europe-small-cap-value",
            ]),
            OutputFormat::Json,
        )
        .await
        .expect("the secondary removal should succeed"),
    );
    assert_eq!(removed["command"], "benchmark.remove_secondary");
    assert_eq!(removed["data"]["removed"], true);
    assert_eq!(
        removed["data"]["removed_reference_ids"],
        serde_json::json!(["msci-europe-small-cap-value"])
    );
    assert!(
        rstock::db::repos::benchmark_secondary_assignment_repo::find_by_subject(
            &db, "tracked", "XFAKE1"
        )
        .await
        .unwrap()
        .is_empty()
    );
}

#[tokio::test]
async fn a_secondary_assignment_never_replaces_the_primary_and_keeps_the_candidate_identity() {
    let db = setup_test_db().await;
    insert_asset(&db, "XFAKE1", "Tracked fixture", "stock", "EUR").await;
    assign_primary(
        &db,
        "tracked",
        "XFAKE1",
        "msci-acwi-small-cap-value-net-eur",
    )
    .await;

    // A candidate secondary assignment keeps the exact class identity of the
    // candidate: its ISIN and its vehicle.
    let candidate = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&[
                "rstock",
                "benchmark",
                "assign-secondary-candidate",
                "--identifier",
                CANDIDATE_ISIN,
                "--vehicle",
                "fund",
                "msci-europe-small-cap-value",
            ]),
            OutputFormat::Json,
        )
        .await
        .expect("the candidate secondary assignment should succeed"),
    );
    assert_eq!(candidate["data"]["subject_type"], "fund_candidate");
    assert_eq!(
        candidate["data"]["subject_id"],
        format!("fund:{CANDIDATE_ISIN}")
    );
    assert_eq!(candidate["data"]["role"], "secondary");
    assert_eq!(candidate["data"]["creates_holding"], false);

    // The same reference under another vehicle is a different candidate and
    // keeps its own assignments.
    let other_vehicle = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&[
                "rstock",
                "benchmark",
                "remove-secondary-candidate",
                "--identifier",
                CANDIDATE_ISIN,
                "--vehicle",
                "etf",
            ]),
            OutputFormat::Json,
        )
        .await
        .expect("the removal should succeed"),
    );
    assert_eq!(other_vehicle["data"]["removed"], false);

    // The tracked asset's primary reference is untouched by the candidate
    // work, and the tracked asset's own secondary is a separate record.
    let tracked_secondary = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&[
                "rstock",
                "benchmark",
                "assign-secondary",
                "--ticker",
                "XFAKE1",
                "estr",
            ]),
            OutputFormat::Json,
        )
        .await
        .expect("the secondary assignment should succeed"),
    );
    assert_eq!(tracked_secondary["data"]["subject_type"], "tracked");
    assert_eq!(tracked_secondary["data"]["reference_id"], "estr");
    let primary =
        rstock::db::repos::benchmark_assignment_repo::find_by_subject(&db, "tracked", "XFAKE1")
            .await
            .unwrap()
            .expect("the primary assignment should exist");
    assert_eq!(primary.reference_id, "msci-acwi-small-cap-value-net-eur");

    // The same reference cannot be the primary and the secondary of one
    // subject: one series compared with itself is not a second comparison.
    let rejected = run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            "XFAKE1",
            "msci-acwi-small-cap-value-net-eur",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("the primary reference must not become a secondary");
    assert!(
        rejected
            .to_string()
            .contains("already the primary reference"),
        "the refusal must name the primary reference: {rejected}"
    );

    // Every secondary of one subject can be removed in one explicit act, and
    // the removed identities are reported.
    let removed = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&[
                "rstock",
                "benchmark",
                "remove-secondary",
                "--ticker",
                "XFAKE1",
            ]),
            OutputFormat::Json,
        )
        .await
        .expect("the secondary removal should succeed"),
    );
    assert_eq!(removed["data"]["removed"], true);
    assert_eq!(
        removed["data"]["removed_reference_ids"],
        serde_json::json!(["estr"])
    );
    assert!(
        rstock::db::repos::benchmark_secondary_assignment_repo::find_by_subject(
            &db, "tracked", "XFAKE1"
        )
        .await
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        rstock::db::repos::benchmark_secondary_assignment_repo::find_by_subject(
            &db,
            "fund_candidate",
            &format!("fund:{CANDIDATE_ISIN}")
        )
        .await
        .unwrap()
        .len(),
        1
    );
}

// --- Comparison and display ---

#[tokio::test]
async fn a_secondary_comparison_is_measured_beside_the_primary_comparison() {
    let (db, sources) = fund_fixture().await;
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            FUND_ISIN,
            SECONDARY_REFERENCE,
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary assignment should succeed");

    let result = analyze(&db, &sources).await;
    let envelope = analysis_envelope(&result);

    // The primary comparison keeps its own role and its own reference.
    let primary = &envelope["primary_benchmark_comparison"];
    assert_eq!(primary["role"], "primary");
    assert_eq!(primary["reference_id"], PRIMARY_REFERENCE);
    assert_eq!(primary["availability"]["status"], "available");

    // The secondary comparison carries its own role, identity, conventions,
    // dates, and wealth series.
    let secondaries = envelope["secondary_benchmark_comparisons"]
        .as_array()
        .expect("the secondary comparisons should be a list");
    assert_eq!(secondaries.len(), 1);
    let secondary = &secondaries[0];
    assert_eq!(secondary["role"], "secondary");
    assert_eq!(secondary["reference_id"], SECONDARY_REFERENCE);
    assert_eq!(secondary["reference_name"], "Verified Secondary Reference");
    assert_eq!(secondary["source"], "test_source");
    assert_eq!(secondary["source_identifier"], SECONDARY_IDENTIFIER);
    assert_eq!(secondary["currency"], "EUR");
    assert_eq!(secondary["native_currency"], "EUR");
    assert_eq!(secondary["return_basis"], "total_return");
    assert_eq!(secondary["adjustment"], "dividends_reinvested_once");
    assert_eq!(secondary["hedge_policy"], "Unhedged");
    assert_eq!(secondary["availability"]["status"], "available");
    assert_eq!(secondary["start_date"], "2024-11-01");
    assert_eq!(secondary["end_date"], "2025-01-08");
    assert_eq!(secondary["purpose"]["universe"], "test universe");
    assert_eq!(secondary["purpose"]["verification_state"], "verified");
    assert_eq!(
        secondary["purpose"]["official_comparator"],
        "Stated official comparator"
    );
    assert!(secondary["fund_wealth"].as_array().unwrap().len() > 1);
    assert!(secondary["reference_wealth"].as_array().unwrap().len() > 1);

    // The secondary comparison has its own standard rows.
    let periods = secondary["standard_periods"].as_array().unwrap();
    assert_eq!(periods.len(), 5);
    assert_eq!(periods[4]["period"], "since_comparison_start");
    assert_eq!(periods[4]["start_date"], "2024-11-01");
    assert_eq!(periods[4]["end_date"], "2025-01-08");

    // The human output shows the role, the reference identity, the dates, and
    // the aligned wealth graph of the secondary comparison.
    let human = analysis_human(&result);
    assert!(human.contains("Secondary comparisons (diagnostics only"));
    assert!(human.contains("Secondary Fixture Fund assigned secondary comparison: available"));
    assert!(human.contains(
        "Secondary Fixture Fund secondary reference: Verified Secondary Reference [verified-secondary-reference]"
    ));
    assert!(human.contains("Aligned wealth graph (EUR wealth rebased to 100)"));
    assert!(human.contains("Comparison dates: 01-11-2024 – 08-01-2025"));
}

#[tokio::test]
async fn a_missing_secondary_input_leaves_the_primary_comparison_intact() {
    let (db, mut sources) = fund_fixture().await;
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            FUND_ISIN,
            SECONDARY_REFERENCE,
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary assignment should succeed");
    // The secondary reference stops reporting, so its comparison cannot be
    // measured.
    sources
        .historical_prices
        .insert(SECONDARY_IDENTIFIER.to_owned(), Vec::new());

    let result = analyze(&db, &sources).await;
    let envelope = analysis_envelope(&result);

    let primary = &envelope["primary_benchmark_comparison"];
    assert_eq!(primary["availability"]["status"], "available");
    assert_eq!(primary["reference_id"], PRIMARY_REFERENCE);
    assert!(primary["fund_return_pct"].as_f64().is_some());

    let secondary = &envelope["secondary_benchmark_comparisons"][0];
    assert_eq!(secondary["role"], "secondary");
    assert_eq!(secondary["availability"]["status"], "unavailable");
    assert!(
        secondary["availability"]["reason"]
            .as_str()
            .unwrap()
            .contains("coverage is incomplete"),
        "the missing secondary input must be named: {:?}",
        secondary["availability"]["reason"]
    );
    assert!(
        !secondary["reference_limitations"]
            .as_array()
            .unwrap()
            .is_empty(),
        "the missing secondary input keeps its own scoped limitation"
    );
    assert!(secondary["fund_return_pct"].is_null());

    // The fund-only facts of the analysis keep their own outcomes.
    assert!(envelope["ytd"].is_object());
    assert!(envelope["one_year"].is_object());
}

#[tokio::test]
async fn the_approved_secondary_targets_are_selected_but_never_used_as_unverified_series() {
    let (db, sources) = fund_fixture().await;
    for reference_id in ["msci-europe-small-cap-value", "estr"] {
        let assigned = common::parse_json_envelope(
            &run_benchmark(
                &db,
                benchmark_command(&[
                    "rstock",
                    "benchmark",
                    "assign-secondary",
                    "--ticker",
                    FUND_ISIN,
                    reference_id,
                ]),
                OutputFormat::Json,
            )
            .await
            .expect("the approved secondary target should be selectable"),
        );
        assert_eq!(assigned["data"]["reference_id"], reference_id);
        assert_eq!(assigned["data"]["role"], "secondary");
        assert_eq!(
            assigned["data"]["definition"]["verification_state"],
            "target_only"
        );
    }

    let result = analyze(&db, &sources).await;
    let envelope = analysis_envelope(&result);
    let secondaries = envelope["secondary_benchmark_comparisons"]
        .as_array()
        .expect("the secondary comparisons should be a list");
    assert_eq!(secondaries.len(), 2);
    for secondary in secondaries {
        assert_eq!(secondary["role"], "secondary");
        assert_eq!(
            secondary["availability"]["status"], "unavailable",
            "an unverified source must not become a usable comparison"
        );
        assert!(secondary["availability"]["reason"]
            .as_str()
            .unwrap()
            .contains("unverified production mapping"));
        assert!(secondary["fund_return_pct"].is_null());
        assert!(secondary["fund_wealth"].as_array().unwrap().is_empty());
    }
    // The complete primary comparison is unaffected by the two unverified
    // secondary targets.
    assert_eq!(
        envelope["primary_benchmark_comparison"]["availability"]["status"],
        "available"
    );
}

#[tokio::test]
async fn a_secondary_replacement_compares_the_whole_window_without_a_splice() {
    let (db, sources) = fund_fixture().await;
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            FUND_ISIN,
            SECONDARY_REFERENCE,
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary assignment should succeed");

    // The explicit replacement removes the first secondary and selects the
    // second one.
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "remove-secondary",
            "--ticker",
            FUND_ISIN,
            "--reference-id",
            SECONDARY_REFERENCE,
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary removal should succeed");
    let replacement = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&[
                "rstock",
                "benchmark",
                "assign-secondary",
                "--ticker",
                FUND_ISIN,
                REPLACEMENT_REFERENCE,
            ]),
            OutputFormat::Json,
        )
        .await
        .expect("the secondary assignment should succeed"),
    );
    assert_eq!(replacement["data"]["reference_id"], REPLACEMENT_REFERENCE);
    assert_eq!(replacement["data"]["retrospective"], true);

    let result = analyze(&db, &sources).await;
    let envelope = analysis_envelope(&result);
    let secondaries = envelope["secondary_benchmark_comparisons"]
        .as_array()
        .expect("the secondary comparisons should be a list");
    assert_eq!(secondaries.len(), 1);
    assert_eq!(secondaries[0]["reference_id"], REPLACEMENT_REFERENCE);
    // The new secondary is compared over the whole retrospective window: the
    // comparison opens at the first observation and closes at the end, with
    // no effective-dated chain and no spliced series.
    assert_eq!(secondaries[0]["start_date"], "2024-11-01");
    assert_eq!(secondaries[0]["end_date"], "2025-01-08");
    let wealth = secondaries[0]["reference_wealth"].as_array().unwrap();
    assert_eq!(wealth[0]["date"], "2024-11-01");
    assert!((wealth[0]["wealth"].as_f64().unwrap() - 100.0).abs() < 1e-9);
}

#[tokio::test]
async fn the_secondary_standard_rows_reuse_the_shared_period_support() {
    let (db, sources) = fund_fixture().await;
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            FUND_ISIN,
            SECONDARY_REFERENCE,
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary assignment should succeed");

    let result = analyze(&db, &sources).await;
    let envelope = analysis_envelope(&result);
    let secondary = &envelope["secondary_benchmark_comparisons"][0];

    // The reported rows are the shared standard-period support of the same
    // aligned series, not a second calculation.
    let expected = compute_standard_benchmark_periods(
        &weekday_series(100.0, 0.10),
        &weekday_series(25.0, 0.02),
        "2024-11-01",
        "2025-01-08",
    )
    .expect("the shared standard periods should be computable");
    let reported = secondary["standard_periods"]
        .as_array()
        .expect("the standard periods should be a list");
    assert_eq!(expected.len(), reported.len());
    for (period, row) in expected.iter().zip(reported.iter()) {
        assert_eq!(
            row["period"],
            serde_json::to_value(period.period).expect("the period should serialize")
        );
        assert_eq!(
            row["requested_start_date"],
            optional_date(period.requested_start_date.clone())
        );
        assert_eq!(row["start_date"], optional_date(period.start_date.clone()));
        assert_eq!(row["end_date"], optional_date(period.end_date.clone()));
        assert_eq!(
            row["fund_return_pct"],
            fact_value(&period.subject_return_pct),
            "the subject return must come from the shared support"
        );
        assert_eq!(
            row["reference_return_pct"],
            fact_value(&period.reference_return_pct),
            "the reference return must come from the shared support"
        );
    }
}

/// The value of one comparison fact.
fn fact_value(fact: &ComparisonFact<f64>) -> Value {
    match fact {
        ComparisonFact::Available { value } => {
            serde_json::json!({ "status": "available", "value": value })
        }
        ComparisonFact::Unavailable { reason } => {
            serde_json::json!({ "status": "unavailable", "reason": reason })
        }
        ComparisonFact::NotApplicable { reason } => {
            serde_json::json!({ "status": "not_applicable", "reason": reason })
        }
    }
}

/// One optional date as its JSON value.
fn optional_date(date: Option<String>) -> Value {
    match date {
        Some(date) => Value::String(date),
        None => Value::Null,
    }
}

#[tokio::test]
async fn the_secondary_comparisons_of_one_exact_class_include_the_candidate_record() {
    let (db, sources) = fund_fixture().await;
    // The exact class record of the same fund selects a secondary reference.
    // The Tracked asset and the Fund candidate of one exact ISIN describe the
    // same investment, so both records contribute their secondary selections.
    let candidate = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&[
                "rstock",
                "benchmark",
                "assign-secondary-candidate",
                "--identifier",
                FUND_ISIN,
                "--vehicle",
                "fund",
                SECONDARY_REFERENCE,
            ]),
            OutputFormat::Json,
        )
        .await
        .expect("the candidate secondary assignment should succeed"),
    );
    assert_eq!(candidate["data"]["subject_id"], format!("fund:{FUND_ISIN}"));

    let result = analyze(&db, &sources).await;
    let envelope = analysis_envelope(&result);
    let secondaries = envelope["secondary_benchmark_comparisons"]
        .as_array()
        .expect("the secondary comparisons should be a list");
    assert_eq!(secondaries.len(), 1);
    assert_eq!(secondaries[0]["reference_id"], SECONDARY_REFERENCE);
    assert_eq!(secondaries[0]["role"], "secondary");
    assert_eq!(secondaries[0]["availability"]["status"], "available");

    // The same reference selected by both records is reported once.
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            FUND_ISIN,
            SECONDARY_REFERENCE,
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the tracked secondary assignment should succeed");
    let result = analyze(&db, &sources).await;
    let envelope = analysis_envelope(&result);
    assert_eq!(
        envelope["secondary_benchmark_comparisons"]
            .as_array()
            .expect("the secondary comparisons should be a list")
            .len(),
        1
    );
}

#[tokio::test]
async fn the_fund_comparison_reports_the_secondary_comparisons_of_each_side() {
    let (db, sources) = fund_fixture().await;
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            FUND_ISIN,
            SECONDARY_REFERENCE,
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary assignment should succeed");

    let market_data = market_data_at(&sources, TODAY);
    let mut output = Vec::new();
    rstock::cli::run_command_with_writer(
        &db,
        &market_data,
        rstock::cli::Commands::Compare(rstock::cli::CompareArgs {
            command: rstock::cli::CompareCommands::Funds {
                code_a: FUND_CODE.to_owned(),
                code_b: FUND_CODE_B.to_owned(),
                period: rstock::cli::CorrelationPeriod::ThirtyDays,
            },
        }),
        OutputFormat::Json,
        &mut output,
    )
    .await
    .expect("the fund comparison should succeed");
    let envelope = common::parse_json_envelope(&output);
    assert_eq!(envelope["command"], "compare.funds");

    // The side with the secondary comparison keeps it, and the other side
    // keeps its own empty list: one side's secondary never becomes the other
    // side's comparison.
    assert_eq!(
        envelope["data"]["fund_a"]["primary_benchmark_comparison"]["role"],
        "primary"
    );
    let fund_a_secondaries = envelope["data"]["fund_a"]["secondary_benchmark_comparisons"]
        .as_array()
        .expect("the secondary comparisons should be a list");
    assert_eq!(fund_a_secondaries.len(), 1);
    assert_eq!(fund_a_secondaries[0]["role"], "secondary");
    assert_eq!(fund_a_secondaries[0]["reference_id"], SECONDARY_REFERENCE);
    assert_eq!(fund_a_secondaries[0]["availability"]["status"], "available");
    assert!(
        envelope["data"]["fund_b"]["secondary_benchmark_comparisons"]
            .as_array()
            .expect("the secondary comparisons should be a list")
            .is_empty()
    );
    assert_eq!(
        envelope["data"]["fund_b"]["primary_benchmark_comparison"]["reference_id"],
        PRIMARY_REFERENCE_B
    );

    let mut human = Vec::new();
    let result = rstock::services::fund_comparison::compare_funds(
        &db,
        &market_data,
        FUND_CODE,
        FUND_CODE_B,
        rstock::models::FundComparisonPeriod {
            label: "30D",
            days: 30,
        },
    )
    .await
    .expect("the fund comparison should succeed");
    rstock::cli::display::write_fund_comparison_reference_context(&mut human, &result)
        .expect("the human output should be written");
    let human = String::from_utf8(human).expect("the human output should be UTF-8");
    assert!(human.contains("secondary comparisons (diagnostics only"));
    assert!(human.contains("assigned secondary comparison: available"));
    assert!(human.contains("secondary reference: Verified Secondary Reference"));
}

// --- Portfolio isolation ---

#[tokio::test]
async fn a_secondary_assignment_never_enters_the_exposure_matched_weights() {
    let (db, sources) = exposure_fixture().await;
    let before = exposure_comparison(&db, &sources).await;

    // A third reference becomes the secondary comparison of one held asset.
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-secondary",
            "--ticker",
            "XFAKE1",
            "ref-gamma",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary assignment should succeed");

    let after = exposure_comparison(&db, &sources).await;
    assert_eq!(
        before, after,
        "a secondary assignment must not change the exposure-matched comparison"
    );
    assert!(
        !after
            .references
            .iter()
            .any(|reference| reference.reference_id == "ref-gamma"),
        "the secondary reference must never become a contributing reference"
    );
    let alpha = after
        .references
        .iter()
        .find(|reference| reference.reference_id == "ref-alpha")
        .expect("the primary reference must stay in place");
    assert_eq!(alpha.assets, vec!["XFAKE1".to_owned(), "XFAKE3".to_owned()]);
    assert!((alpha.first_weight_pct.unwrap() - 60.0).abs() < 1e-9);
    let beta = after
        .references
        .iter()
        .find(|reference| reference.reference_id == "ref-beta")
        .expect("the second primary reference must stay visible");
    assert_eq!(beta.assets, vec!["XFAKE2".to_owned()]);
    assert!((beta.first_weight_pct.unwrap() - 40.0).abs() < 1e-9);
}

// --- Definition deletion ---

#[tokio::test]
async fn a_definition_used_by_a_secondary_assignment_cannot_be_deleted() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, "blend-component", "Blend Component", "XREFA").await;
    let asset_id = insert_asset(&db, "XFAKE1", "Blend fixture", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 1.0, 10.0, 0.0).await;
    insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 1.0).await;
    insert_portfolio_asset_snapshot(&db, "2025-01-02", asset_id, 1.0, 100.0, 100.0, 1.0).await;
    insert_portfolio_snapshot(&db, "2025-01-08", 110.0, 1.0).await;
    insert_portfolio_asset_snapshot(&db, "2025-01-08", asset_id, 1.0, 110.0, 110.0, 1.0).await;
    let mut sources = MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XREFA".to_owned(),
        series_between("2025-01-02", "2025-01-08", 100.0, 0.0),
    );
    sources.series_qualifications.insert(
        ("test_source".to_owned(), "XREFA".to_owned()),
        daily_qualification(),
    );

    run_blend(
        &db,
        &sources,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Secondary Blend",
            "--component",
            "blend-component:100",
            "--start",
            "02-01-2025",
        ]),
    )
    .await
    .expect("the blend should be saved");

    let assigned = common::parse_json_envelope(
        &run_benchmark(
            &db,
            benchmark_command(&[
                "rstock",
                "benchmark",
                "assign-secondary",
                "--ticker",
                "XFAKE1",
                "custom-blend-secondary-blend",
            ]),
            OutputFormat::Json,
        )
        .await
        .expect("the blend should be assignable as a secondary"),
    );
    assert_eq!(
        assigned["data"]["reference_id"],
        "custom-blend-secondary-blend"
    );

    let rejected = run_blend(
        &db,
        &sources,
        blend_command(&["rstock", "blend", "delete", "custom-blend-secondary-blend"]),
    )
    .await
    .expect_err("a definition used by a secondary assignment must not be deleted");
    assert!(
        rejected.to_string().contains("secondary assignment"),
        "the refusal must name the secondary assignment: {rejected}"
    );
    assert!(
        rstock::db::repos::custom_blend_repo::exists(&db, "custom-blend-secondary-blend")
            .await
            .unwrap()
    );

    // Removing the secondary assignment releases the definition, and the
    // deletion preserves the ledger, the NAV history, and the cached
    // observations.
    run_benchmark(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "remove-secondary",
            "--ticker",
            "XFAKE1",
            "--reference-id",
            "custom-blend-secondary-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the secondary removal should succeed");

    let deleted = common::parse_json_envelope(
        &run_blend(
            &db,
            &sources,
            blend_command(&["rstock", "blend", "delete", "custom-blend-secondary-blend"]),
        )
        .await
        .expect("an unused blend should be deleted"),
    );
    assert_eq!(deleted["data"]["deleted"], true);
    assert!(
        !rstock::db::repos::custom_blend_repo::exists(&db, "custom-blend-secondary-blend")
            .await
            .unwrap()
    );
    assert!(benchmark_reference::Entity::find_by_id("blend-component")
        .one(&db)
        .await
        .unwrap()
        .is_some());
    assert!(
        !benchmark_reference_price::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .is_empty(),
        "the cached reference prices must survive"
    );
    assert_eq!(transaction::Entity::find().all(&db).await.unwrap().len(), 1);
    assert_eq!(
        portfolio_history::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        portfolio_asset_history::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        2
    );
}

// --- Exposure fixture ---

/// A three-asset portfolio with one primary reference per asset and a third
/// reference that is only used as a secondary comparison.
async fn exposure_fixture() -> (DatabaseConnection, MockMarketDataSources) {
    let db = setup_test_db().await;
    let alpha = insert_asset(&db, "XFAKE1", "Alpha Stock", "stock", "EUR").await;
    let beta = insert_asset(&db, "XFAKE2", "Beta Stock", "stock", "EUR").await;
    let gamma = insert_asset(&db, "XFAKE3", "Gamma Stock", "stock", "EUR").await;
    for (id, name, identifier) in [
        ("ref-alpha", "Verified Alpha Reference", "ALPHA"),
        ("ref-beta", "Verified Beta Reference", "BETA"),
        ("ref-gamma", "Verified Gamma Reference", "GAMMA"),
    ] {
        insert_verified_reference(&db, id, name, identifier).await;
    }
    assign_primary(&db, "tracked", "XFAKE1", "ref-alpha").await;
    assign_primary(&db, "tracked", "XFAKE2", "ref-beta").await;
    assign_primary(&db, "tracked", "XFAKE3", "ref-alpha").await;

    insert_nav_snapshot(&db, "2025-01-05", 0.0, 0.0, 0.0, 100.0).await;
    insert_nav_snapshot(&db, "2025-01-06", 1000.0, 1000.0, 10.0, 100.0).await;
    insert_nav_snapshot(&db, "2025-01-07", 1100.0, 1100.0, 10.0, 110.0).await;
    insert_nav_snapshot(&db, "2025-01-08", 1100.0, 1100.0, 10.0, 110.0).await;
    insert_exposure(&db, "2025-01-06", alpha, 6.0, 100.0, 600.0).await;
    insert_exposure(&db, "2025-01-06", beta, 4.0, 100.0, 400.0).await;
    insert_exposure(&db, "2025-01-06", gamma, 0.0, 100.0, 0.0).await;
    insert_exposure(&db, "2025-01-07", alpha, 6.0, 110.0, 660.0).await;
    insert_exposure(&db, "2025-01-07", beta, 4.0, 110.0, 440.0).await;
    insert_exposure(&db, "2025-01-08", alpha, 6.0, 110.0, 660.0).await;
    insert_exposure(&db, "2025-01-08", beta, 4.0, 110.0, 440.0).await;

    let mut sources = MockMarketDataSources::new();
    for identifier in ["ALPHA", "BETA", "GAMMA"] {
        sources.historical_prices.insert(
            identifier.to_owned(),
            vec![
                ("2025-01-06".to_owned(), 100.0),
                ("2025-01-07".to_owned(), 110.0),
                ("2025-01-08".to_owned(), 110.0),
            ],
        );
        sources.series_qualifications.insert(
            ("test_source".to_owned(), identifier.to_owned()),
            daily_qualification(),
        );
    }
    (db, sources)
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
    .expect("the NAV snapshot should persist");
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
    .expect("the per-asset exposure should persist");
}

/// The ready Complete NAV snapshots of the exposure fixture.
fn ready_snapshots() -> Vec<PortfolioSnapshot> {
    vec![
        PortfolioSnapshot {
            date: "2025-01-05".to_owned(),
            asset_value: 0.0,
            total_value: 0.0,
            outstanding_shares: 0.0,
            nav: 100.0,
        },
        PortfolioSnapshot {
            date: "2025-01-06".to_owned(),
            asset_value: 1000.0,
            total_value: 1000.0,
            outstanding_shares: 10.0,
            nav: 100.0,
        },
        PortfolioSnapshot {
            date: "2025-01-07".to_owned(),
            asset_value: 1100.0,
            total_value: 1100.0,
            outstanding_shares: 10.0,
            nav: 110.0,
        },
        PortfolioSnapshot {
            date: "2025-01-08".to_owned(),
            asset_value: 1100.0,
            total_value: 1100.0,
            outstanding_shares: 10.0,
            nav: 110.0,
        },
    ]
}

async fn exposure_comparison(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
) -> rstock::models::ExposureBenchmarkComparison {
    let market_data = market_data_at(sources, NaiveDate::from_ymd_opt(2025, 1, 9).unwrap());
    compute_exposure_matched_comparison(db, &market_data, &ready_snapshots())
        .await
        .expect("the exposure comparison should be computable")
}
