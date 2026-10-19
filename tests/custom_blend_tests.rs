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
    Set, Statement,
};

use common::{market_data_at, setup_test_db, MockMarketDataSources};

const REFERENCE_A: &str = "x-blend-reference-a";
const REFERENCE_B: &str = "x-blend-reference-b";
const SOURCE: &str = "test_source";

/// A component definition whose qualification is verified and daily.
async fn insert_verified_reference(db: &DatabaseConnection, id: &str, identifier: &str) {
    benchmark_reference::ActiveModel {
        id: Set(id.to_owned()),
        display_name: Set(format!("Blend reference {identifier}")),
        instrument_ticker: Set(identifier.to_owned()),
        source: Set(SOURCE.to_owned()),
        source_identifier: Set(identifier.to_owned()),
        currency: Set("EUR".to_owned()),
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

#[tokio::test]
async fn saved_blend_round_trips_through_public_commands_with_explicit_conventions() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date");

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
    assert_eq!(market_component["hedge_policy"], "Unhedged");
    assert_eq!(market_component["verification_state"], "verified");
    let cash_component = &components[1];
    assert_eq!(cash_component["component_key"], "cash");
    assert_eq!(
        cash_component["component_reference_id"],
        serde_json::Value::Null
    );
    assert_eq!(cash_component["is_cash"], true);
    assert_eq!(cash_component["target_weight"], 0.4);
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
        "Admitted window: 2025-01-02 through 2025-02-04; basis: explicit_later_start",
        "Blend reference XREFA [x-blend-reference-a]",
        "target weight: 60.00%",
        "return basis: total_return",
        "hedge policy: Unhedged",
        "verification: verified",
        "Synthetic cash [cash]",
        "target weight: 40.00%",
    ] {
        assert!(
            human.contains(expected),
            "the human output omitted {expected}: {human}"
        );
    }

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
async fn invalid_weights_duplicates_and_nested_blends_are_rejected_without_saving_anything() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    insert_verified_reference(&db, REFERENCE_B, "XREFB").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date");

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
async fn the_default_admission_window_is_the_full_comparison_history_and_rejects_atomically() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date");

    let rejected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Full History Blend",
            "--component",
            "x-blend-reference-a:100",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a component without the full comparison history must be rejected");
    let reason = rejected.to_string();
    assert!(
        reason.contains("no source observation on 2000-01-03"),
        "the rejection must name the first uncovered date: {reason}"
    );

    assert!(
        custom_blend_repo::find_all(&db).await.unwrap().is_empty(),
        "a rejected admission must not persist a partial blend"
    );
    assert!(
        !benchmark_reference::Entity::find_by_id("custom-blend-full-history-blend")
            .one(&db)
            .await
            .unwrap()
            .is_some(),
        "a rejected admission must not persist a definition row"
    );
}

#[tokio::test]
async fn an_explicit_later_start_is_admitted_with_honest_earlier_period_limitations() {
    let db = setup_test_db().await;
    insert_verified_reference(&db, REFERENCE_A, "XREFA").await;
    let sources = offline_sources(
        flat_observations("2025-01-02", "2025-02-04", 100.0),
        flat_observations("2025-01-02", "2025-02-04", 100.0),
    );
    let today = NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date");

    let created = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "create",
            "--name",
            "Later Start Blend",
            "--component",
            "x-blend-reference-a:100",
            "--start",
            "02-01-2025",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("an explicitly later start must be admitted");
    let created = common::parse_json_envelope(&created);
    assert_eq!(created["data"]["admission_basis"], "explicit_later_start");
    assert_eq!(created["data"]["admitted_start_date"], "2025-01-02");

    let preview = run_blend(
        &db,
        &sources,
        today,
        blend_command(&[
            "rstock",
            "blend",
            "preview",
            "custom-blend-later-start-blend",
        ]),
        OutputFormat::Json,
    )
    .await
    .expect("the blend should preview");
    let preview = common::parse_json_envelope(&preview);
    assert_eq!(preview["command"], "blend.preview");
    let limitation = preview["data"]["earlier_period_limitation"]
        .as_str()
        .expect("an explicit later start must disclose its earlier-period limitation");
    assert!(limitation.contains("2025-01-02"), "{limitation}");
    assert!(
        limitation.contains("unavailable"),
        "the limitation must state that earlier periods are unavailable: {limitation}"
    );
    assert_eq!(preview["data"]["anchor_date"], "2025-01-02");
    assert_eq!(preview["data"]["rebased_to"], 100.0);
    assert_eq!(preview["data"]["window_start_date"], "2025-01-02");
    assert_eq!(preview["data"]["window_end_date"], "2025-02-04");
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
    let today = NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date");

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
async fn an_in_use_blend_cannot_be_deleted_and_deletion_preserves_history() {
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
    let today = NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date");

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
    custom_blend_repo::insert_strategy_selection(
        &db,
        "custom-blend-assigned-blend",
        "2025-02-05T00:00:00Z",
    )
    .await
    .unwrap();
    let selected = run_blend(
        &db,
        &sources,
        today,
        blend_command(&["rstock", "blend", "delete", "custom-blend-assigned-blend"]),
        OutputFormat::Json,
    )
    .await
    .expect_err("a selected strategy must not be deleted");
    assert!(
        selected.to_string().contains("selected Strategy"),
        "the refusal must name the selected Strategy: {selected}"
    );

    custom_blend_repo::delete_strategy_selection(&db)
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
    let today = NaiveDate::from_ymd_opt(2025, 2, 5).expect("valid clock date");

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
        "Window: 2025-01-30 through 2025-02-04; anchor close: 2025-01-30; rebased to 100",
        "Last observation: 2025-02-04",
        "Reset rule: monthly",
        "Weight convention: Beginning-of-interval target weights with return-driven drift",
        "Cash convention: Synthetic 0% return cash",
        "Accounting: Monthly target resets are frictionless synthetic accounting",
        "Blend reference XREFA [x-blend-reference-a]",
        "return basis: total_return",
        "hedge policy: Unhedged",
        "universe: Equities",
        "source: Fake offline source",
        "verification: verified",
        "reset at the 2025-01-31 close, governing the interval from 2025-02-03",
        "2025-01-30  100.0000",
        "2025-02-04  110.2500",
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
    assert_eq!(json["data"]["window_start_date"], "2025-01-30");
    assert_eq!(json["data"]["window_end_date"], "2025-02-04");
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
    assert_eq!(json["data"]["components"][0]["hedge_policy"], "Unhedged");
    assert_eq!(
        json["data"]["components"][0]["verification_state"],
        "verified"
    );
    assert_eq!(
        json["data"]["components"][0]["source_provenance"],
        "Fake offline source"
    );
}
