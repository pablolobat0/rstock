#![allow(dead_code)]

mod common;

use clap::Parser;
use rstock::{
    cli::{commands::benchmarks::write_output, output::OutputFormat, Cli},
    db::{
        entities::{benchmark_assignment, portfolio_history, transaction},
        repos::benchmark_reference_repo,
    },
};
use sea_orm::EntityTrait;

#[tokio::test]
async fn dispatch_persists_candidate_and_tracked_assignments_without_ledger_or_nav_changes() {
    let db = common::setup_test_db().await;
    let market_data = common::market_data_at(
        &common::MockMarketDataSources::default(),
        chrono::NaiveDate::from_ymd_opt(2026, 10, 18).unwrap(),
    );
    let asset_id = common::insert_asset(&db, "XFAKE1", "Fake", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-01", 1.0, 10.0, 0.0).await;
    let command = |args: &[&str]| Cli::try_parse_from(args).unwrap().command;
    rstock::cli::run_command(
        &db,
        &market_data,
        command(&[
            "rstock",
            "benchmark",
            "assign",
            "XFAKE1",
            "msci-world-net-eur-unhedged",
            "--override-selection",
        ]),
        OutputFormat::Json,
    )
    .await
    .unwrap();
    rstock::cli::run_command(
        &db,
        &market_data,
        command(&[
            "rstock",
            "benchmark",
            "assign-candidate",
            "LU1598719752",
            "--vehicle",
            "fund",
            "msci-acwi-small-cap-value-net-eur",
        ]),
        OutputFormat::Json,
    )
    .await
    .unwrap();
    assert_eq!(
        benchmark_reference_repo::find_assignment(&db, "tracked", "XFAKE1")
            .await
            .unwrap(),
        Some(("msci-world-net-eur-unhedged".to_owned(), true))
    );
    assert_eq!(
        benchmark_reference_repo::find_assignment(&db, "fund_candidate", "fund:LU1598719752")
            .await
            .unwrap()
            .unwrap()
            .0,
        "msci-acwi-small-cap-value-net-eur"
    );
    assert_eq!(transaction::Entity::find().all(&db).await.unwrap().len(), 1);
    assert_eq!(
        portfolio_history::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        0
    );
    let assignments = benchmark_assignment::Entity::find().all(&db).await.unwrap();
    assert_eq!(assignments.len(), 2);
    rstock::cli::run_command(
        &db,
        &market_data,
        command(&["rstock", "benchmark", "remove", "XFAKE1"]),
        OutputFormat::Human,
    )
    .await
    .unwrap();
    assert!(
        benchmark_reference_repo::find_assignment(&db, "tracked", "XFAKE1")
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(
        benchmark_reference_repo::find_assignment(&db, "fund_candidate", "fund:LU1598719752")
            .await
            .unwrap()
            .unwrap()
            .0,
        "msci-acwi-small-cap-value-net-eur"
    );
    assert_eq!(transaction::Entity::find().all(&db).await.unwrap().len(), 1);
}

#[test]
fn catalog_output_is_writer_injected_and_json_retains_unknown_verification() {
    let reference = serde_json::json!({"id":"target","role":"Investment universe","universe":"Unknown","return_basis_qualification":"Unknown","currency":"Unknown","hedge_policy":"Unknown","source_provenance":"unverified","history_boundary":"Unknown","verification_state":"target_only"});
    let mut bytes = Vec::new();
    write_output(
        &mut bytes,
        OutputFormat::Json,
        "benchmark.show",
        &reference,
        "Target",
    )
    .unwrap();
    let output = common::parse_json_envelope(&bytes);
    assert_eq!(output["data"]["verification_state"], "target_only");
    assert_eq!(output["data"]["history_boundary"], "Unknown");

    let mut human = Vec::new();
    write_output(
        &mut human,
        OutputFormat::Human,
        "benchmark.show",
        &reference,
        "Target with unknown source and history",
    )
    .unwrap();
    assert_eq!(
        String::from_utf8(human).unwrap(),
        "Target with unknown source and history\n"
    );
}
