#![allow(dead_code)]

mod common;

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use clap::Parser;
use migration::{Migrator, MigratorTrait};
use rstock::{
    cli::{output::OutputFormat, BlendCommands, Cli},
    db::{
        entities::benchmark_reference,
        repos::{benchmark_assignment_repo, custom_blend_repo},
    },
    services::market_data::SeriesQualification,
};
use sea_orm::{
    ActiveModelTrait, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, EntityTrait,
    Set, Statement, TransactionTrait,
};

use common::{market_data_at, setup_test_db, MockMarketDataSources};

const REFERENCE_A: &str = "x-blend-reference-a";
const REFERENCE_B: &str = "x-blend-reference-b";
const SOURCE: &str = "test_source";

/// The clock the modern fixtures use.
fn clock() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date")
}

/// A component definition whose qualification is verified and daily.
async fn insert_verified_reference(db: &DatabaseConnection, id: &str, identifier: &str) {
    insert_reference_with_currency(db, id, identifier, "EUR").await;
}

/// A component definition in an explicit currency.
async fn insert_reference_with_currency(
    db: &DatabaseConnection,
    id: &str,
    identifier: &str,
    currency: &str,
) {
    benchmark_reference::ActiveModel {
        id: Set(id.to_owned()),
        display_name: Set(format!("Blend reference {identifier}")),
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

fn qualification() -> SeriesQualification {
    SeriesQualification {
        currency: "EUR".to_owned(),
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

/// The weekday observations of one fake component between two dates, minus one
/// excluded interior weekday.
fn weekday_observations_excluding(start: &str, end: &str, excluded: &str) -> Vec<(String, f64)> {
    weekday_observations(
        start,
        end,
        |date| {
            if date == excluded {
                0.0
            } else {
                100.0
            }
        },
    )
    .into_iter()
    .filter(|(date, _)| date != excluded)
    .collect()
}

/// A flat component price series over one window.
fn flat_observations(start: &str, end: &str, price: f64) -> Vec<(String, f64)> {
    weekday_observations(start, end, |_| price)
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

/// The modern portfolio fixture: an invested Complete closing snapshot from
/// `first_date` through the effective comparison end `end_date`.
async fn insert_invested_portfolio(db: &DatabaseConnection, first_date: &str, end_date: &str) {
    let asset_id = common::insert_asset(db, "XFAKEINV", "Invested fixture", "stock", "EUR").await;
    common::insert_portfolio_snapshot(db, first_date, 100.0, 1.0).await;
    common::insert_portfolio_asset_snapshot(db, first_date, asset_id, 1.0, 100.0, 100.0, 1.0).await;
    common::insert_portfolio_snapshot(db, end_date, 110.0, 1.0).await;
    common::insert_portfolio_asset_snapshot(db, end_date, asset_id, 1.0, 110.0, 110.0, 1.0).await;
}

/// The Monetary-only portfolio fixture: a portfolio snapshot whose only
/// holding is Monetary, so no invested comparison boundary exists.
async fn insert_monetary_only_portfolio(db: &DatabaseConnection, date: &str) {
    let asset_id =
        common::insert_monetary_fund_asset(db, "XFAKEMON", "Monetary fixture", "EUR", "XFON1")
            .await;
    common::insert_portfolio_snapshot(db, date, 100.0, 1.0).await;
    common::insert_portfolio_asset_snapshot(db, date, asset_id, 1.0, 1.0, 100.0, 1.0).await;
}

/// Parses one blend command.
fn blend_command(args: &[&str]) -> BlendCommands {
    match Cli::try_parse_from(args)
        .expect("the command should parse")
        .command
    {
        rstock::cli::Commands::Blend(arguments) => arguments.command,
        other => panic!("expected a blend command, got {other:?}"),
    }
}

/// Runs one blend command against the fixed clock and captures its output.
async fn run_blend(
    db: &DatabaseConnection,
    sources: &MockMarketDataSources,
    today: NaiveDate,
    command: BlendCommands,
    format: OutputFormat,
) -> anyhow::Result<Vec<u8>> {
    let market_data = market_data_at(sources, today);
    let mut output = Vec::new();
    rstock::cli::run_blend_command_with_writer(db, &market_data, command, format, &mut output)
        .await?;
    Ok(output)
}

/// The wealth of one dated point of a JSON preview.
fn wealth_at(preview: &serde_json::Value, index: usize) -> (String, f64) {
    let point = &preview["data"]["wealth"][index];
    (
        point["date"].as_str().expect("a dated point").to_owned(),
        point["wealth"].as_f64().expect("a numeric wealth"),
    )
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "expected {expected}, got {actual}"
    );
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

#[tokio::test]
async fn saved_blend_round_trips_through_public_commands_with_explicit_conventions() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    let output = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Global Sixty Forty",
            "--component",
            "x-blend-reference-a:60",
            "--component",
            "cash:40",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");
    let envelope = common::parse_json_envelope(&output);
    assert_eq!(envelope["command"], "blend.create");
    assert_eq!(envelope["data"]["id"], "custom-blend-global-sixty-forty");
    assert_eq!(envelope["data"]["display_name"], "Global Sixty Forty");
    assert_eq!(envelope["data"]["currency"], "EUR");
    assert_eq!(envelope["data"]["reset_rule"], "monthly");
    assert_eq!(envelope["data"]["admission_basis"], "explicit_later_start");
    assert_eq!(envelope["data"]["requested_start_date"], "2025-01-02");
    assert_eq!(envelope["data"]["requested_end_date"], "2025-02-04");
    assert_eq!(envelope["data"]["admitted_start_date"], "2025-01-02");
    assert_eq!(envelope["data"]["admitted_end_date"], "2025-02-04");
    assert_eq!(envelope["data"]["weight_tolerance"], 1e-4);
    let components = envelope["data"]["components"].as_array().unwrap();
    assert_eq!(components.len(), 2);
    let market_component = &components[0];
    assert_eq!(market_component["component_key"], REFERENCE_A);
    assert_eq!(market_component["component_reference_id"], REFERENCE_A);
    assert_eq!(market_component["target_weight"], 0.6);
    assert_eq!(market_component["target_weight_percent"], 60.0);
    assert_eq!(market_component["is_cash"], false);
    assert_eq!(market_component["currency"], "EUR");
    assert_eq!(market_component["return_basis"], "total_return");
    assert_eq!(market_component["adjustment"], "dividends_reinvested_once");
    assert_eq!(market_component["hedge_policy"], "Unhedged");
    assert_eq!(market_component["source"], SOURCE);
    assert_eq!(market_component["source_identifier"], "XREFA");
    assert_eq!(market_component["cadence"], "daily");
    assert_eq!(market_component["verification_state"], "verified");
    let cash_component = &components[1];
    assert_eq!(cash_component["component_key"], "cash");
    assert_eq!(
        cash_component["component_reference_id"],
        serde_json::Value::Null
    );
    assert_eq!(cash_component["is_cash"], true);
    assert_eq!(cash_component["target_weight"], 0.4);
    assert_eq!(cash_component["cadence"], "synthetic");
    assert!(cash_component["return_basis"]
        .as_str()
        .unwrap()
        .contains("0% return"));

    let shown = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "show", "custom-blend-global-sixty-forty"]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be shown");
    let shown = common::parse_json_envelope(&shown);
    assert_eq!(shown["command"], "blend.show");
    assert_eq!(shown["data"], envelope["data"]);

    let human = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "show", "custom-blend-global-sixty-forty"]),
        OutputFormat::Human,
    )
    .await
    .expect("the blend should render");
    let human = String::from_utf8(human).unwrap();
    for expected in [
        "Custom blend 'Global Sixty Forty' [custom-blend-global-sixty-forty]",
        "reset rule: monthly",
        "Requested window: 02-01-2025 through 04-02-2025; basis: explicit_later_start",
        "Admitted window: 02-01-2025 through 04-02-2025",
        "Blend reference XREFA [x-blend-reference-a]",
        "target weight: 60.00%",
        "return basis: total_return",
        "adjustment: dividends_reinvested_once",
        "hedge policy: Unhedged",
        "source: test_source",
        "source identifier: XREFA",
        "cadence: daily",
        "verification: verified",
        "Synthetic cash [cash]",
        "target weight: 40.00%",
    ] {
        assert!(
            human.contains(expected),
            "the human output omitted {expected}: {human}"
        );
    }
    assert!(
        !human.contains("2025-01-02"),
        "the human output must not show stored ISO dates: {human}"
    );

    let listed = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "list"]),
        OutputFormat::Json,
    )
    .await
    .expect("the blends should be listed");
    let listed = common::parse_json_envelope(&listed);
    assert_eq!(listed["command"], "blend.list");
    assert_eq!(listed["data"]["blends"].as_array().unwrap().len(), 1);
    assert_eq!(
        listed["data"]["blends"][0]["id"],
        "custom-blend-global-sixty-forty"
    );
}

#[tokio::test]
async fn the_default_admission_window_is_the_portfolio_comparison_boundary() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_invested_portfolio(&db, "2025-01-02", "2025-02-04").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    let created = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Portfolio Window Blend",
            "--component",
            "x-blend-reference-a:100",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be admitted against the portfolio window");
    let created = common::parse_json_envelope(&created);
    assert_eq!(
        created["data"]["admission_basis"],
        "full_portfolio_comparison_history"
    );
    assert_eq!(created["data"]["requested_start_date"], "2025-01-02");
    assert_eq!(created["data"]["requested_end_date"], "2025-02-04");
    assert_eq!(created["data"]["admitted_start_date"], "2025-01-02");
    assert_eq!(created["data"]["admitted_end_date"], "2025-02-04");
}

#[tokio::test]
async fn a_pre_2000_portfolio_boundary_is_not_clamped_to_a_fabricated_date() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_invested_portfolio(&db, "1998-06-01", "1998-06-04").await;
    let sources = offline_sources(
        flat_observations("1998-06-01", "1998-06-04", 100.0),
        flat_observations("1998-06-01", "1998-06-04", 100.0),
    );
    let today = NaiveDate::from_ymd_opt(1998, 6, 5).expect("valid clock date");

    let created = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Pre Two Thousand Blend",
            "--component",
            "x-blend-reference-a:100",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("a pre-2000 portfolio boundary must be admitted as it is");
    let created = common::parse_json_envelope(&created);
    assert_eq!(created["data"]["requested_start_date"], "1998-06-01");
    assert_eq!(created["data"]["requested_end_date"], "1998-06-04");
    assert_eq!(created["data"]["admitted_start_date"], "1998-06-01");
    assert_eq!(created["data"]["admitted_end_date"], "1998-06-04");
}

#[tokio::test]
async fn an_empty_or_monetary_only_portfolio_has_no_admission_window() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    let empty = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Empty Portfolio Blend",
            "--component",
            "x-blend-reference-a:100",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("an empty portfolio has no comparison window");
    assert!(
        empty
            .to_string()
            .contains("no ready complete comparison history"),
        "the refusal must name the missing portfolio history: {empty}"
    );

    insert_monetary_only_portfolio(&db, "2025-01-02").await;
    let monetary = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Monetary Only Blend",
            "--component",
            "x-blend-reference-a:100",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a Monetary-only portfolio has no invested comparison window");
    assert!(
        monetary
            .to_string()
            .contains("no ready complete comparison history"),
        "the refusal must name the missing invested history: {monetary}"
    );

    assert!(
        custom_blend_repo::find_all(&db).await.unwrap().is_empty(),
        "a refused admission must not persist a blend"
    );
}

#[tokio::test]
async fn invalid_weights_duplicates_and_nested_blends_are_rejected_without_saving_anything() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    let short = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Short Blend",
            "--component",
            "x-blend-reference-a:60",
            "--component",
            "x-blend-reference-b:30",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("weights that do not total 100% must be rejected");
    assert!(short.to_string().contains("must total 100%"));

    let leveraged = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Leveraged Blend",
            "--component",
            "x-blend-reference-a:150",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("leveraged weights must be rejected");
    assert!(leveraged.to_string().contains("must total 100%"));

    let short_position = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Short Position Blend",
            "--component",
            "x-blend-reference-a:-10",
            "--component",
            "x-blend-reference-b:110",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a short component must be rejected");
    assert!(short_position
        .to_string()
        .contains("long-only and admits no short"));

    let duplicate = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Duplicate Blend",
            "--component",
            "x-blend-reference-a:50",
            "--component",
            "x-blend-reference-a:50",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a repeated component must be rejected");
    assert!(duplicate.to_string().contains("duplicate component"));

    assert!(
        Cli::try_parse_from([
            "rstock",
            "blend",
            "create",
            "--name",
            "Not Finite",
            "--component",
            "x-blend-reference-a:NaN",
        ])
        .is_err(),
        "a non-finite weight must be rejected by the command"
    );

    let saved = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Nested Parent",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the parent blend should be saved");
    let saved = common::parse_json_envelope(&saved);
    let parent_id = saved["data"]["id"].as_str().unwrap().to_owned();

    let nested = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Nested Child",
            "--component",
            &format!("{parent_id}:100"),
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a nested blend must be rejected");
    assert!(nested
        .to_string()
        .contains("nested blends are not supported"));

    let blends = custom_blend_repo::find_all(&db).await.unwrap();
    assert_eq!(blends.len(), 1, "only the parent blend may be saved");
    assert_eq!(blends[0].id, parent_id);
}

#[tokio::test]
async fn a_weekend_request_resolves_to_the_preceding_completed_weekday_close() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    // 2025-01-11 is a Saturday, so the window opens at the 2025-01-10 close.
    let sources = offline_sources(
        flat_observations("2025-01-10", "2025-02-04", 100.0),
        flat_observations("2025-01-10", "2025-02-04", 100.0),
    );
    let today = clock();

    let created = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Weekend Start Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "11-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("a weekend request resolves by calendar rule");
    let created = common::parse_json_envelope(&created);
    assert_eq!(created["data"]["requested_start_date"], "2025-01-11");
    assert_eq!(created["data"]["admitted_start_date"], "2025-01-10");
    assert_eq!(created["data"]["admitted_end_date"], "2025-02-04");
}

#[tokio::test]
async fn a_weekend_end_resolves_to_the_preceding_completed_weekday_close() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-01-31", 100.0),
        flat_observations("2025-01-13", "2025-01-31", 100.0),
    );
    // 2025-02-03 is a Monday, so the last completed date is the 2025-02-02
    // Sunday, which resolves to the 2025-01-31 Friday close.
    let today = NaiveDate::from_ymd_opt(2025, 2, 3).expect("valid clock date");

    let mut created = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Weekend End Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("a weekend end resolves by calendar rule");
    let created = common::parse_json_envelope(&created);
    assert_eq!(created["data"]["requested_end_date"], "2025-02-02");
    assert_eq!(created["data"]["admitted_end_date"], "2025-01-31");
}

#[tokio::test]
async fn a_missing_required_close_is_rejected_even_when_every_feed_misses_it() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    // The Friday close of the requested start is missing from every feed.
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let today = clock();

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Missing Friday Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "11-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a missing required Friday close must be rejected");
    let reason = rejected.to_string();
    assert!(
        reason.contains("no anchor close on 2025-01-10"),
        "the refusal must name the missing required close: {reason}"
    );
    assert!(
        !reason.contains("market closure"),
        "a missing close must never be reported as a market closure: {reason}"
    );
    assert!(custom_blend_repo::find_all(&db).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_missing_end_close_is_rejected() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let today = clock();
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-03", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let missing_end = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Missing End Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a component without the required end close must be rejected");
    assert!(
        missing_end
            .to_string()
            .contains("no end close on 2025-02-04"),
        "the refusal must name the missing end close: {missing_end}"
    );
    assert!(custom_blend_repo::find_all(&db).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_internal_weekday_gap_is_rejected() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let today = clock();
    let sources = offline_sources(
        weekday_observations_excluding("2025-01-13", "2025-02-04", "2025-01-17"),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let internal_gap = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Internal Gap Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("an internal weekday gap must be rejected");
    assert!(
        internal_gap
            .to_string()
            .contains("no observation on 2025-01-17"),
        "the refusal must name the missing interior observation: {internal_gap}"
    );
    assert!(custom_blend_repo::find_all(&db).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_incomplete_base_currency_conversion_is_rejected_at_the_anchor() {
    let db = setup_test_db().await;
    benchmark_reference::ActiveModel {
        id: Set(REFERENCE_A.to_owned()),
        display_name: Set("Blend reference XREFA".to_owned()),
        instrument_ticker: Set("XREFA".to_owned()),
        source: Set(SOURCE.to_owned()),
        source_identifier: Set("XREFA".to_owned()),
        currency: Set("USD".to_owned()),
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
    .insert(&db)
    .await
    .unwrap();
    let mut sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    sources.series_qualifications.insert(
        (SOURCE.to_owned(), "XREFA".to_owned()),
        SeriesQualification {
            currency: "USD".to_owned(),
            return_basis: "total_return".to_owned(),
            adjustment: "dividends_reinvested_once".to_owned(),
            hedge_policy: "Unhedged".to_owned(),
            cadence: "daily".to_owned(),
        },
    );
    // No USD/EUR rate is available, so the Base currency conversion of the
    // component cannot supply the required closes.
    let today = clock();
    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Missing Conversion Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a component without its Base currency conversion must be rejected");
    let reason = rejected.to_string();
    assert!(
        reason.contains("no anchor close on 2025-01-13"),
        "the refusal must name the missing anchor close: {reason}"
    );
    assert!(
        reason.contains("Base currency conversion is missing or incomplete"),
        "the refusal must name the incomplete conversion: {reason}"
    );
}

#[tokio::test]
async fn an_unverified_or_changed_component_convention_is_rejected() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Convention Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");

    let mut changed: benchmark_reference::ActiveModel =
        benchmark_reference::Entity::find_by_id(REFERENCE_A)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .into();
    changed.source_identifier = Set("XREFA-ALT".to_owned());
    changed.update(&db).await.unwrap();
    let changed_convention = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-convention-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a changed lookup identity must invalidate the saved blend");
    assert!(
        changed_convention
            .to_string()
            .contains("no longer matches the source, lookup identity"),
        "the refusal must name the changed convention: {changed_convention}"
    );

    let mut unverified: benchmark_reference::ActiveModel =
        benchmark_reference::Entity::find_by_id(REFERENCE_A)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .into();
    unverified.verification_state = Set("target_only".to_owned());
    unverified.update(&db).await.unwrap();
    let unverified_source = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-convention-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("an unverified component must invalidate the saved blend");
    assert!(
        unverified_source
            .to_string()
            .contains("not a verified usable series"),
        "the refusal must name the unverified state: {unverified_source}"
    );
}

#[tokio::test]
async fn preview_compounds_arithmetic_beginning_of_interval_returns_with_drift() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = offline_sources(
        stepped_observations(
            "2025-01-13",
            "2025-01-15",
            &[
                ("2025-01-13", 100.0),
                ("2025-01-14", 110.0),
                ("2025-01-15", 121.0),
            ],
        ),
        stepped_observations(
            "2025-01-13",
            "2025-01-15",
            &[
                ("2025-01-13", 100.0),
                ("2025-01-14", 90.0),
                ("2025-01-15", 90.0),
            ],
        ),
    );
    let today = NaiveDate::from_ymd_opt(2025, 1, 16).expect("valid clock date");

    let created = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Drift Blend",
            "--component",
            "x-blend-reference-a:50",
            "--component",
            "x-blend-reference-b:50",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the drift blend should be saved");
    let created = common::parse_json_envelope(&created);
    assert_eq!(created["data"]["admitted_start_date"], "2025-01-13");
    assert_eq!(created["data"]["admitted_end_date"], "2025-01-15");

    let preview = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "preview", "custom-blend-drift-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect("the drift blend should preview");
    let preview = common::parse_json_envelope(&preview);
    let wealth = preview["data"]["wealth"].as_array().unwrap();
    assert_eq!(wealth.len(), 3);

    // Hand-calculated arithmetic compounding with beginning-of-interval
    // weights: day one nets 0.5 * 10% + 0.5 * -10% = 0%, the drifted weights
    // 55/100 and 45/100 earn 0.55 * 10% = 5.5% on day two, and the drifted
    // weights 60.5/105.5 and 45/105.5 earn 1.46919% on day three.
    assert_eq!(wealth_at(&preview, 0), ("2025-01-13".to_owned(), 100.0));
    assert_eq!(wealth_at(&preview, 1).0, "2025-01-14");
    assert_close(wealth_at(&preview, 1).1, 100.0);
    assert_close(wealth_at(&preview, 2).1, 105.5);
    assert_close(
        preview["data"]["cumulative_return_pct"].as_f64().unwrap(),
        5.5,
    );

    // A weighted average of log returns is a different wealth series: the
    // first interval would compound sqrt(1.1 * 0.9) = 0.994987 instead of 1.
    let log_weighted_first_interval = 100.0 * (1.1_f64 * 0.9_f64).sqrt();
    assert!((log_weighted_first_interval - 99.4987).abs() < 1e-3);
    assert!(
        (wealth_at(&preview, 1).1 - log_weighted_first_interval).abs() > 0.4,
        "the blend must compound arithmetic returns, not weighted log returns"
    );
}

#[tokio::test]
async fn monthly_resets_use_the_close_before_the_new_month_and_keep_a_partial_first_month() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = offline_sources(
        stepped_observations(
            "2025-01-30",
            "2025-02-04",
            &[
                ("2025-01-30", 100.0),
                ("2025-01-31", 110.0),
                ("2025-02-03", 121.0),
                ("2025-02-04", 121.0),
            ],
        ),
        stepped_observations(
            "2025-01-30",
            "2025-02-04",
            &[
                ("2025-01-30", 100.0),
                ("2025-01-31", 100.0),
                ("2025-02-03", 100.0),
                ("2025-02-04", 100.0),
            ],
        ),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Monthly Reset Blend",
            "--component",
            "x-blend-reference-a:50",
            "--component",
            "x-blend-reference-b:50",
            "--start",
            "30-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the monthly reset blend should be saved");

    let preview = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-monthly-reset-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the monthly reset blend should preview");
    let preview = common::parse_json_envelope(&preview);
    let resets = preview["data"]["target_resets"].as_array().unwrap();
    assert_eq!(resets.len(), 1, "only the calendar month boundary resets");
    assert_eq!(resets[0]["reset_date"], "2025-01-31");
    assert_eq!(resets[0]["effective_from_date"], "2025-02-03");
    assert_eq!(
        preview["data"]["anchor_date"], "2025-01-30",
        "a partial first month must not receive a hidden earlier start"
    );

    // 30 January to 31 January: 0.5 * 10% = 5% on the anchored targets.
    assert_close(wealth_at(&preview, 1).1, 105.0);
    // The drifted weights would earn 5.2381% on the February interval; the
    // monthly reset restores the 50/50 targets and earns exactly 5%.
    assert_close(wealth_at(&preview, 2).1, 110.25);
    assert_close(wealth_at(&preview, 3).1, 110.25);
}

#[tokio::test]
async fn an_assigned_blend_cannot_be_deleted_and_deletion_preserves_history() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let asset_id = common::insert_asset(&db, "XFAKE1", "Blend fixture", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 1.0, 10.0, 0.0).await;
    common::insert_portfolio_snapshot(&db, "2025-02-04", 100.0, 1.0).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-02-04", 0.91).await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Assigned Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the assigned blend should be saved");

    let mut assigned = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        match Cli::try_parse_from([
            "rstock",
            "benchmark",
            "assign",
            "--ticker",
            "XFAKE1",
            "custom-blend-assigned-blend",
        ])
        .expect("the assignment command should parse")
        .command
        {
            rstock::cli::Commands::Benchmark(arguments) => arguments.command,
            other => panic!("expected a benchmark command, got {other:?}"),
        },
        OutputFormat::Json,
        &mut assigned,
    )
    .await
    .expect("the blend should be assignable");
    let assigned = common::parse_json_envelope(&assigned);
    assert_eq!(
        assigned["data"]["reference_id"],
        "custom-blend-assigned-blend"
    );

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "delete", "custom-blend-assigned-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect_err("an assigned blend must not be deleted");
    assert!(
        rejected.to_string().contains("active assignment"),
        "the refusal must name the active assignment: {rejected}"
    );
    assert!(
        custom_blend_repo::exists(&db, "custom-blend-assigned-blend")
            .await
            .unwrap()
    );

    benchmark_assignment_repo::delete_by_subject(&db, "tracked", "XFAKE1")
        .await
        .unwrap();
    let output = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "delete", "custom-blend-assigned-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect("an unused blend should be deleted");
    let output = common::parse_json_envelope(&output);
    assert_eq!(output["command"], "blend.delete");
    assert_eq!(output["data"]["deleted"], true);
    assert!(
        !custom_blend_repo::exists(&db, "custom-blend-assigned-blend")
            .await
            .unwrap()
    );

    // The component definition and every price, FX, ledger, and NAV row
    // survive the deletion of the blend.
    assert!(benchmark_reference::Entity::find_by_id(REFERENCE_A)
        .one(&db)
        .await
        .unwrap()
        .is_some());
    let prices = rstock::db::entities::benchmark_reference_price::Entity::find()
        .all(&db)
        .await
        .unwrap();
    assert!(!prices.is_empty(), "component price history must survive");
    assert_eq!(
        rstock::db::entities::daily_exchange_rate::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        rstock::db::entities::transaction::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        rstock::db::entities::portfolio_history::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn a_failed_late_create_write_leaves_no_saved_blend_behind() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        "CREATE TRIGGER fail_component_insert BEFORE INSERT ON benchmark_blend_components \
         BEGIN SELECT RAISE(ABORT, 'injected component write failure'); END",
    ))
    .await
    .unwrap();

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Late Failure Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a failed component write must fail the creation");
    assert!(rejected
        .to_string()
        .contains("injected component write failure"));

    assert!(
        custom_blend_repo::find_all(&db).await.unwrap().is_empty(),
        "a failed write must roll the whole definition back"
    );
    assert!(
        !benchmark_reference::Entity::find_by_id("custom-blend-late-failure-blend")
            .one(&db)
            .await
            .unwrap()
            .is_some(),
        "a failed write must not leave a catalog definition behind"
    );
    assert_eq!(
        rstock::db::entities::benchmark_blend_component::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        0
    );
}

#[tokio::test]
async fn a_failed_final_delete_rolls_the_whole_deletion_back() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let asset_id = common::insert_asset(&db, "XFAKE1", "Delete fixture", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 1.0, 10.0, 0.0).await;
    common::insert_portfolio_snapshot(&db, "2025-02-04", 100.0, 1.0).await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Failed Delete Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");

    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        "CREATE TRIGGER fail_definition_delete BEFORE DELETE ON benchmark_references \
         WHEN OLD.id = 'custom-blend-failed-delete-blend' \
         BEGIN SELECT RAISE(ABORT, 'injected definition delete failure'); END",
    ))
    .await
    .unwrap();

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "delete",
            "custom-blend-failed-delete-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a failed deletion must be reported");
    assert!(rejected.to_string().contains("remains saved"));

    assert!(
        custom_blend_repo::exists(&db, "custom-blend-failed-delete-blend")
            .await
            .unwrap(),
        "a failed deletion must leave the saved definition intact"
    );
    assert_eq!(
        rstock::db::entities::benchmark_blend_component::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1,
        "a failed deletion must restore the component rows"
    );
    assert!(
        benchmark_reference::Entity::find_by_id("custom-blend-failed-delete-blend")
            .one(&db)
            .await
            .unwrap()
            .is_some(),
        "a failed deletion must leave the catalog definition intact"
    );
    assert_eq!(
        rstock::db::entities::transaction::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        rstock::db::entities::portfolio_history::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        !rstock::db::entities::benchmark_reference_price::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .is_empty(),
        "a failed deletion must leave the cached price history intact"
    );
}

#[tokio::test]
async fn an_assignment_created_after_the_usage_check_still_blocks_the_deletion() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Race Guard Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");

    // The assignment exists while the definition is deleted inside one
    // transaction: the reference foreign key is the backstop that keeps the
    // saved definition intact.
    let transaction = db.begin().await.unwrap();
    benchmark_assignment_repo::upsert(
        &transaction,
        "tracked",
        "XFAKE1",
        "custom-blend-race-guard-blend",
        false,
    )
    .await
    .unwrap();
    let deletion =
        custom_blend_repo::delete_blend(&transaction, "custom-blend-race-guard-blend").await;
    assert!(
        deletion.is_err(),
        "an assignment must block the deletion of its definition"
    );
    transaction.rollback().await.unwrap();

    assert!(
        custom_blend_repo::exists(&db, "custom-blend-race-guard-blend")
            .await
            .unwrap(),
        "the blocked deletion must leave the saved definition intact"
    );
}

#[tokio::test]
async fn a_saved_blend_is_immutable_and_cash_never_invents_a_price_or_a_fee() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        stepped_observations(
            "2025-01-13",
            "2025-01-15",
            &[
                ("2025-01-13", 100.0),
                ("2025-01-14", 110.0),
                ("2025-01-15", 110.0),
            ],
        ),
        flat_observations("2025-01-13", "2025-01-15", 100.0),
    );
    let today = NaiveDate::from_ymd_opt(2025, 1, 16).expect("valid clock date");

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Cash Blend",
            "--component",
            "x-blend-reference-a:50",
            "--component",
            "cash:50",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the cash blend should be saved");

    let revision = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Cash Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a saved blend must not be revised");
    assert!(
        revision.to_string().contains("already exists"),
        "the refusal must state that the definition already exists: {revision}"
    );

    let blends = custom_blend_repo::find_all(&db).await.unwrap();
    assert_eq!(blends.len(), 1);
    assert_eq!(blends[0].components.len(), 2);
    assert_eq!(blends[0].components[0].target_weight, 0.5);
    assert_eq!(blends[0].components[1].target_weight, 0.5);
    assert!(blends[0].components[1].is_cash);

    let preview = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "preview", "custom-blend-cash-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect("the cash blend should preview");
    let preview = common::parse_json_envelope(&preview);
    // Cash earns nothing: the blend earns half of the market component's 10%.
    assert_close(wealth_at(&preview, 1).1, 105.0);
    assert_close(wealth_at(&preview, 2).1, 105.0);
    assert!(preview["data"]["accounting_note"]
        .as_str()
        .unwrap()
        .contains("no transactions, fees"));

    // The synthetic cash component never reaches the market data cache.
    let cash_prices = rstock::db::entities::benchmark_reference_price::Entity::find()
        .all(&db)
        .await
        .unwrap();
    assert!(
        cash_prices
            .iter()
            .all(|price| price.reference_id == REFERENCE_A),
        "the synthetic cash component must not cache a price"
    );
    assert_eq!(
        rstock::db::entities::transaction::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        0,
        "a synthetic reset must not create a transaction"
    );
}

#[tokio::test]
async fn the_preview_embeds_component_reset_date_and_convention_provenance() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = offline_sources(
        stepped_observations(
            "2025-01-30",
            "2025-02-04",
            &[
                ("2025-01-30", 100.0),
                ("2025-01-31", 110.0),
                ("2025-02-03", 121.0),
                ("2025-02-04", 121.0),
            ],
        ),
        stepped_observations(
            "2025-01-30",
            "2025-02-04",
            &[
                ("2025-01-30", 100.0),
                ("2025-01-31", 100.0),
                ("2025-02-03", 100.0),
                ("2025-02-04", 100.0),
            ],
        ),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Provenance Blend",
            "--component",
            "x-blend-reference-a:50",
            "--component",
            "x-blend-reference-b:50",
            "--start",
            "30-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the provenance blend should be saved");

    let human = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-provenance-blend",
        ]),
        OutputFormat::Human,
    )
    .await
    .expect("the preview should render");
    let human = String::from_utf8(human).unwrap();
    for expected in [
        "Custom blend preview 'Provenance Blend' [custom-blend-provenance-blend]",
        "Requested window: 30-01-2025 through 04-02-2025; actual anchor 30-01-2025; actual end 04-02-2025; rebased to 100",
        "Admitted window: 30-01-2025 through 04-02-2025",
        "Reset rule: monthly",
        "Weight convention: Beginning-of-interval target weights with return-driven drift",
        "Cash convention: Synthetic 0% return cash",
        "Accounting: Monthly target resets are frictionless synthetic accounting",
        "Blend reference XREFA [x-blend-reference-a]",
        "return basis: total_return",
        "adjustment: dividends_reinvested_once",
        "hedge policy: Unhedged",
        "universe: Equities",
        "source: test_source",
        "source identifier: XREFA",
        "cadence: daily",
        "verification: verified",
        "reset at the 31-01-2025 close, governing the interval from 03-02-2025",
        "30-01-2025  100.0000",
        "04-02-2025  110.2500",
    ] {
        assert!(
            human.contains(expected),
            "the human preview omitted {expected}: {human}"
        );
    }

    let json = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-provenance-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the preview should serialize");
    let json = common::parse_json_envelope(&json);
    assert_eq!(json["command"], "blend.preview");
    assert_eq!(json["data"]["blend_id"], "custom-blend-provenance-blend");
    assert_eq!(json["data"]["reset_rule"], "monthly");
    assert_eq!(
        json["data"]["weight_convention"],
        "Beginning-of-interval target weights with return-driven drift between monthly resets"
    );
    assert_eq!(
        json["data"]["cash_convention"],
        "Synthetic 0% return cash; no market data, no fees, and no transactions"
    );
    assert_eq!(json["data"]["requested_start_date"], "2025-01-30");
    assert_eq!(json["data"]["requested_end_date"], "2025-02-04");
    assert_eq!(json["data"]["anchor_date"], "2025-01-30");
    assert_eq!(json["data"]["last_observation_date"], "2025-02-04");
    assert_eq!(json["data"]["target_resets"][0]["reset_date"], "2025-01-31");
    assert_eq!(
        json["data"]["target_resets"][0]["effective_from_date"],
        "2025-02-03"
    );
    assert_eq!(json["data"]["components"][0]["component_key"], REFERENCE_A);
    assert_eq!(
        json["data"]["components"][0]["return_basis"],
        "total_return"
    );
    assert_eq!(
        json["data"]["components"][0]["adjustment"],
        "dividends_reinvested_once"
    );
    assert_eq!(json["data"]["components"][0]["hedge_policy"], "Unhedged");
    assert_eq!(json["data"]["components"][0]["source"], SOURCE);
    assert_eq!(json["data"]["components"][0]["source_identifier"], "XREFA");
    assert_eq!(json["data"]["components"][0]["cadence"], "daily");
    assert_eq!(
        json["data"]["components"][0]["verification_state"],
        "verified"
    );
    assert_eq!(
        json["data"]["components"][0]["source_provenance"],
        "Fake offline source"
    );
}

#[tokio::test]
async fn a_malformed_cached_observation_is_a_hard_error_not_an_unavailable_fact() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let today = clock();

    rstock::db::entities::benchmark_reference_price::ActiveModel {
        reference_id: Set(REFERENCE_A.to_owned()),
        date: Set("2025-01-13".to_owned()),
        closing_price: Set(-5.0),
        is_api_failure: Set(false),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Malformed Cache Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a malformed cached observation must be a hard error");
    assert!(
        rejected
            .to_string()
            .contains("invalid persisted benchmark reference price"),
        "the failure must name the malformed cached observation: {rejected}"
    );
    assert!(custom_blend_repo::find_all(&db).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_failed_source_can_be_retried_by_a_later_command() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let mut sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    sources.failing_price_sources.insert("XREFA".to_owned());
    let today = clock();

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Retried Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a failing source must not admit a blend");
    assert!(
        rejected
            .to_string()
            .contains("no anchor close on 2025-01-13"),
        "the refusal must name the missing required close: {rejected}"
    );
    assert!(custom_blend_repo::find_all(&db).await.unwrap().is_empty());

    sources.failing_price_sources.remove("XREFA");
    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Retried Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("a later command must retry the failed source and admit the blend");
    assert_eq!(custom_blend_repo::find_all(&db).await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_cash_only_blend_round_trips_through_create_show_and_preview() {
    let db = setup_test_db().await;
    let sources = offline_sources(Vec::new(), Vec::new());
    let today = clock();

    let mut created = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Cash Only Blend",
            "--component",
            "cash:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("a cash-only blend should be saved");
    let created = common::parse_json_envelope(&created);
    assert_eq!(created["data"]["id"], "custom-blend-cash-only-blend");
    assert_eq!(created["data"]["admitted_start_date"], "2025-01-13");
    assert_eq!(created["data"]["admitted_end_date"], "2025-02-04");
    assert_eq!(created["data"]["components"].as_array().unwrap().len(), 1);
    assert_eq!(created["data"]["components"][0]["is_cash"], true);

    let mut shown = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "show", "custom-blend-cash-only-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect("the cash-only blend should be shown");
    let shown = common::parse_json_envelope(&shown);
    assert_eq!(shown["data"], created["data"]);

    let mut preview = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "preview", "custom-blend-cash-only-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect("the cash-only blend should preview");
    let preview = common::parse_json_envelope(&preview);
    let wealth = preview["data"]["wealth"].as_array().unwrap();
    assert_eq!(
        wealth.len(),
        17,
        "the resolved completed weekdays of the window"
    );
    assert_eq!(wealth[0]["date"], "2025-01-13");
    assert_eq!(wealth[16]["date"], "2025-02-04");
    for point in wealth {
        assert_eq!(point["wealth"], 100.0, "cash earns nothing");
    }
    assert_eq!(preview["data"]["cumulative_return_pct"], 0.0);
    assert_eq!(
        preview["data"]["target_resets"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        preview["data"]["target_resets"][0]["reset_date"],
        "2025-01-31"
    );
    assert_eq!(
        preview["data"]["target_resets"][0]["effective_from_date"],
        "2025-02-03"
    );

    let mut human = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "preview", "custom-blend-cash-only-blend"]),
        OutputFormat::Human,
    )
    .await
    .expect("the cash-only preview should render");
    let human = String::from_utf8(human).unwrap();
    for expected in [
        "Custom blend preview 'Cash Only Blend' [custom-blend-cash-only-blend]",
        "Requested window: 13-01-2025 through 04-02-2025; actual anchor 13-01-2025; actual end 04-02-2025",
        "Cumulative return: 0.0000%",
        "reset at the 31-01-2025 close, governing the interval from 03-02-2025",
        "13-01-2025  100.0000",
        "04-02-2025  100.0000",
    ] {
        assert!(
            human.contains(expected),
            "the human preview omitted {expected}: {human}"
        );
    }
    assert_no_iso_dates(&human);

    assert_eq!(
        sources.historical_source_call_count(),
        0,
        "a synthetic cash-only blend must not request any market data"
    );
    assert_eq!(
        rstock::db::entities::transaction::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        0,
        "a synthetic cash blend must not create a transaction"
    );
}

#[tokio::test]
async fn a_changed_component_adjustment_is_rejected() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Adjustment Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");

    let mut changed: benchmark_reference::ActiveModel =
        benchmark_reference::Entity::find_by_id(REFERENCE_A)
            .one(&db)
            .await
            .unwrap()
            .unwrap()
            .into();
    changed.adjustment_qualification = Set("split_adjusted".to_owned());
    changed.update(&db).await.unwrap();

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-adjustment-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a changed adjustment must invalidate the saved blend");
    let reason = rejected.to_string();
    assert!(
        reason.contains("no longer matches the source, lookup identity"),
        "the refusal must name the changed convention: {reason}"
    );
    assert!(
        reason.contains("adjustment"),
        "the refusal must name the adjustment: {reason}"
    );
}

#[tokio::test]
async fn a_changed_component_cadence_is_rejected() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let mut sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Cadence Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");

    sources.series_qualifications.insert(
        (SOURCE.to_owned(), "XREFA".to_owned()),
        SeriesQualification {
            cadence: "weekly".to_owned(),
            ..qualification()
        },
    );
    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "preview", "custom-blend-cadence-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a changed cadence must invalidate the saved blend");
    let reason = rejected.to_string();
    assert!(
        reason.contains("no longer reports the cadence recorded for this blend"),
        "the refusal must name the changed cadence: {reason}"
    );
    assert!(
        reason.contains("weekly"),
        "the refusal must name the reported cadence: {reason}"
    );
}

#[tokio::test]
async fn a_missing_or_mismatched_source_qualification_is_rejected_at_admission() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let mut sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let today = clock();

    sources
        .series_qualifications
        .remove(&(SOURCE.to_owned(), "XREFA".to_owned()));
    let missing = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Missing Qualification Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a component without a source qualification must be rejected");
    assert!(
        missing.to_string().contains(
            "has no verified return-basis, adjustment, currency, hedge, and cadence qualification"
        ),
        "the refusal must name the missing qualification: {missing}"
    );
    assert!(custom_blend_repo::find_all(&db).await.unwrap().is_empty());

    sources.series_qualifications.insert(
        (SOURCE.to_owned(), "XREFA".to_owned()),
        SeriesQualification {
            currency: "USD".to_owned(),
            ..qualification()
        },
    );
    let mismatched = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Mismatched Qualification Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a component whose qualification mismatches must be rejected");
    assert!(
        mismatched.to_string().contains(
            "does not match its declared currency, adjustment, return basis, or hedge policy"
        ),
        "the refusal must name the mismatched convention: {mismatched}"
    );
    assert!(custom_blend_repo::find_all(&db).await.unwrap().is_empty());
}

#[tokio::test]
async fn a_missing_base_currency_rate_at_the_end_is_rejected_after_an_earlier_valid_rate() {
    let db = setup_test_db().await;
    insert_reference_with_currency(&db, REFERENCE_A, "XREFA", "USD").await;
    let mut sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    sources.series_qualifications.insert(
        (SOURCE.to_owned(), "XREFA".to_owned()),
        SeriesQualification {
            currency: "USD".to_owned(),
            ..qualification()
        },
    );
    // The rate is valid for the earlier dates and stops before the window end.
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        weekday_observations("2025-01-13", "2025-01-24", |_| 0.9),
    );
    let today = clock();

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Missing Rate End Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a component without its end conversion must be rejected");
    let reason = rejected.to_string();
    assert!(
        reason.contains("no end close on 2025-02-04"),
        "the refusal must name the missing end close: {reason}"
    );
    assert!(
        reason.contains("Base currency conversion is missing or incomplete"),
        "the refusal must name the incomplete conversion: {reason}"
    );
    assert!(custom_blend_repo::find_all(&db).await.unwrap().is_empty());
}

#[tokio::test]
async fn an_internal_gap_with_a_complete_base_currency_conversion_is_rejected() {
    let db = setup_test_db().await;
    insert_reference_with_currency(&db, REFERENCE_A, "XREFA", "USD").await;
    let mut sources = offline_sources(
        weekday_observations_excluding("2025-01-13", "2025-02-04", "2025-01-17"),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    sources.series_qualifications.insert(
        (SOURCE.to_owned(), "XREFA".to_owned()),
        SeriesQualification {
            currency: "USD".to_owned(),
            ..qualification()
        },
    );
    // The conversion is complete over the whole window.
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        weekday_observations("2025-01-13", "2025-02-04", |_| 0.9),
    );
    let today = clock();

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Internal Gap Conversion Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("an internal gap must be rejected");
    let reason = rejected.to_string();
    assert!(
        reason.contains("no observation on 2025-01-17"),
        "the refusal must name the missing interior observation: {reason}"
    );
    assert!(
        !reason.contains("Base currency conversion is missing or incomplete"),
        "a complete conversion must not be reported as incomplete: {reason}"
    );
}

#[tokio::test]
async fn a_later_clock_extends_the_preview_without_changing_the_saved_definition() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-11", 100.0),
        flat_observations("2025-01-13", "2025-02-11", 100.0),
    );

    let created = run_blend(
        &db,
        &sources,
        clock(),
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Extending Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");
    let created = common::parse_json_envelope(&created);
    assert_eq!(created["data"]["admitted_end_date"], "2025-02-04");

    let later = NaiveDate::from_ymd_opt(2025, 2, 12).expect("valid clock date");
    let mut preview = run_blend(
        &db,
        &sources,
        later,
        blend_command(&["rstock", "blend", "preview", "custom-blend-extending-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect("the preview should reach the new completed end");
    let preview = common::parse_json_envelope(&preview);
    assert_eq!(preview["data"]["requested_end_date"], "2025-02-11");
    assert_eq!(preview["data"]["last_observation_date"], "2025-02-11");
    assert_eq!(preview["data"]["admitted_end_date"], "2025-02-04");

    let mut shown = run_blend(
        &db,
        &sources,
        later,
        blend_command(&["rstock", "blend", "show", "custom-blend-extending-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect("the definition should be shown");
    let shown = common::parse_json_envelope(&shown);
    assert_eq!(
        shown["data"], created["data"],
        "the saved definition must not change with the clock"
    );
}

#[tokio::test]
async fn a_later_clock_without_the_new_end_close_is_rejected() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );

    run_blend(
        &db,
        &sources,
        clock(),
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Missing New End Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");

    let later = NaiveDate::from_ymd_opt(2025, 2, 12).expect("valid clock date");
    let rejected = run_blend(
        &db,
        &sources,
        later,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-missing-new-end-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a missing new end close must reject the preview");
    let reason = rejected.to_string();
    assert!(
        reason.contains("no end close on 2025-02-11"),
        "the refusal must name the missing new end close: {reason}"
    );
}

#[tokio::test]
async fn the_human_output_never_shows_a_stored_iso_date() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-13", "2025-02-04", 100.0),
        flat_observations("2025-01-13", "2025-02-04", 100.0),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Human Date Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "13-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");

    for command in [
        blend_command(&["rstock", "blend", "show", "custom-blend-human-date-blend"]),
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-human-date-blend",
        ]),
        blend_command(&["rstock", "blend", "list"]),
    ] {
        let output = run_blend(&db, &sources, today, command, OutputFormat::Human)
            .await
            .expect("the command should render");
        let human = String::from_utf8(output).unwrap();
        assert_no_iso_dates(&human);
    }

    let mut human = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-human-date-blend",
        ]),
        OutputFormat::Human,
    )
    .await
    .expect("the preview should render");
    let human = String::from_utf8(human).unwrap();
    assert!(
        human.contains("the comparison was explicitly started on 13-01-2025"),
        "the limitation must show the date in the user-facing format: {human}"
    );

    let mut json = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-human-date-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the preview should serialize");
    let json = common::parse_json_envelope(&json);
    let limitation = json["data"]["earlier_period_limitation"]
        .as_str()
        .expect("the machine limitation must stay");
    assert!(
        limitation.contains("2025-01-13"),
        "the machine limitation keeps the stored date: {limitation}"
    );
}

#[tokio::test]
async fn the_blend_migration_preserves_accounting_and_cache_history_in_both_directions() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("in-memory SQLite");
    let boundary = Migrator::migrations()
        .iter()
        .position(|migration| migration.name().contains("add_custom_blends"))
        .expect("the blend migration should be registered") as u32;
    Migrator::up(&db, Some(boundary))
        .await
        .expect("the schema before the blend migration");

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
        .expect("the additive blend migration");
    let kind: String = db
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT kind AS value FROM benchmark_references WHERE id = 'migration-reference'",
        ))
        .await
        .expect("the kind column should be readable")
        .expect("the reference row should exist")
        .try_get("", "value")
        .expect("the kind value");
    assert_eq!(kind, "reference");

    Migrator::down(&db, Some(1))
        .await
        .expect("the blend migration should roll back");
    Migrator::up(&db, Some(1))
        .await
        .expect("the blend migration should apply again");
    assert!(Migrator::get_pending_migrations(&db)
        .await
        .expect("pending migrations")
        .is_empty());

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
async fn a_populated_blend_refuses_the_destructive_migration_rollback() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = clock();

    run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Populated Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should be saved");

    let rollback = Migrator::down(&db, Some(1)).await;
    assert!(
        rollback.is_err(),
        "a destructive rollback must be refused while saved blends exist"
    );
    let error = rollback.expect_err("the rollback must fail").to_string();
    assert!(
        error.contains("saved Custom blends exist"),
        "the refusal must explain the refusal: {error}"
    );

    assert!(
        custom_blend_repo::exists(&db, "custom-blend-populated-blend")
            .await
            .unwrap(),
        "the refused rollback must keep the saved blend intact"
    );
    assert!(
        benchmark_reference::Entity::find_by_id("custom-blend-populated-blend")
            .one(&db)
            .await
            .unwrap()
            .is_some(),
        "the refused rollback must not convert the blend into an ordinary reference"
    );
    assert_eq!(
        rstock::db::entities::benchmark_blend_component::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );

    // The same refusal holds for an assigned blend, and the assignment
    // survives the refused rollback untouched.
    benchmark_assignment_repo::upsert(
        &db,
        "tracked",
        "XFAKE1",
        "custom-blend-populated-blend",
        false,
    )
    .await
    .unwrap();
    assert!(Migrator::down(&db, Some(1)).await.is_err());
    assert_eq!(
        benchmark_assignment_repo::find_by_reference(&db, "custom-blend-populated-blend")
            .await
            .unwrap()
            .len(),
        1,
        "the refused rollback must keep the assignment intact"
    );
}
