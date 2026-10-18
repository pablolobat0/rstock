#![allow(dead_code)]

mod common;

use clap::Parser;
use migration::{Migrator, MigratorTrait};
use rstock::{
    cli::{output::OutputFormat, Cli},
    db::{
        entities::{
            asset, benchmark_candidate, benchmark_reference, benchmark_reference_price,
            daily_asset_price, daily_exchange_rate, portfolio_asset_history, portfolio_history,
            transaction,
        },
        repos::{benchmark_assignment_repo, benchmark_reference_repo},
    },
};
use sea_orm::EntityTrait;
use sea_orm::QueryOrder;

const BENCHMARK_MIGRATION_BOUNDARY: u32 = 2;

#[tokio::test]
async fn catalog_and_candidate_provenance_render_from_persisted_outcomes() {
    let db = common::setup_test_db().await;
    let mut output = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&["rstock", "benchmark", "list"]),
        OutputFormat::Human,
        &mut output,
    )
    .await
    .unwrap();
    let human = String::from_utf8(output).unwrap();
    for expected in [
        "MSCI World",
        "Investment universe",
        "Developed-market large/mid-cap equities",
        "Net total return",
        "EUR",
        "Unhedged",
        "production source identifier unknown",
        "Index history boundary unknown",
        "target_only",
        "Cobas International P Acc EUR",
        "MSCI Europe Total Net Return",
        "Bloomberg Europe Developed Markets Large & Mid Cap Net Return Index EUR",
        "EDMNE Index",
        "iShares Emerging Markets S EUR Acc",
        "MSCI Emerging Markets, Net Returns (EUR)",
    ] {
        assert!(
            human.contains(expected),
            "catalog human output omitted {expected}: {human}"
        );
    }
    assert!(human.contains("Unassigned Tracked assets:"));
    assert!(human.contains("Exact Fund candidate comparator provenance"));

    let mut json = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&["rstock", "benchmark", "show", "msci-world-net-eur-unhedged"]),
        OutputFormat::Json,
        &mut json,
    )
    .await
    .unwrap();
    let envelope = common::parse_json_envelope(&json);
    assert_eq!(envelope["command"], "benchmark.show");
    for field in [
        "role",
        "universe",
        "return_basis_qualification",
        "currency",
        "hedge_policy",
        "source_provenance",
        "history_boundary",
        "verification_state",
    ] {
        assert!(!envelope["data"][field].is_null(), "missing {field}");
    }
    assert_eq!(envelope["data"]["verification_state"], "target_only");
    assert_eq!(envelope["data"]["source_identifier"], "");
    let assignment_semantics = envelope["data"]["assignment_semantics"].as_str().unwrap();
    assert!(assignment_semantics.contains("retrospectively"));

    let mut show_human = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&["rstock", "benchmark", "show", "msci-world-net-eur-unhedged"]),
        OutputFormat::Human,
        &mut show_human,
    )
    .await
    .unwrap();
    assert!(String::from_utf8(show_human)
        .unwrap()
        .contains(assignment_semantics));

    let mut list_json = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&["rstock", "benchmark", "list"]),
        OutputFormat::Json,
        &mut list_json,
    )
    .await
    .unwrap();
    let list = common::parse_json_envelope(&list_json);
    let candidates = list["data"]["candidates"].as_array().unwrap();
    let cobas_europe = candidates
        .iter()
        .find(|candidate| candidate["identifier"] == "LU1598719752")
        .unwrap();
    let cobas_bloomberg = candidates
        .iter()
        .find(|candidate| candidate["identifier"] == "ES0119199018")
        .unwrap();
    assert_eq!(cobas_europe["vehicle"], "fund");
    assert_eq!(
        cobas_europe["official_comparator"],
        "MSCI Europe Total Net Return; exact series code/currency convention unreconciled"
    );
    assert_eq!(
        cobas_europe["official_document_date"],
        "2026-08 factsheet; legal docs 2021/2023"
    );
    assert_eq!(cobas_bloomberg["official_comparator"], "Bloomberg Europe Developed Markets Large & Mid Cap Net Return Index EUR (EDMNE Index); net-of-tax dividend reinvestment; comparison only");
    assert_eq!(
        cobas_bloomberg["official_provenance"],
        "FUND-EXAMPLES.md §5; C1 prospectus updated 2026-06-22; C2 KID 2025-12-31"
    );
    assert_eq!(
        cobas_bloomberg["objective_comparator"],
        serde_json::Value::Null
    );
    assert_eq!(
        cobas_bloomberg["history_boundary"],
        "Manager class performance dates from 2021; index-change effective date/history unknown"
    );
    let vanguard_world = candidates
        .iter()
        .find(|candidate| candidate["identifier"] == "IE00B03HD191")
        .unwrap();
    assert_eq!(vanguard_world["official_comparator"], "MSCI World");
    assert_eq!(
        vanguard_world["official_document_date"],
        "KID dated 2026-09-29; accessed 2026-10-17"
    );
    assert!(vanguard_world["official_provenance"]
        .as_str()
        .unwrap()
        .contains("passive tracking target; exact net/gross return variant unknown"));
    let vanguard_em = candidates
        .iter()
        .find(|candidate| candidate["identifier"] == "IE0031786696")
        .unwrap();
    assert_eq!(vanguard_em["official_comparator"], "MSCI Emerging Markets");
    let carmignac = candidates
        .iter()
        .find(|candidate| candidate["identifier"] == "LU1623762843")
        .unwrap();
    assert_eq!(
        carmignac["official_comparator"],
        "75% ICE BofA Euro Corporate Index + 25% ICE BofA Euro High Yield Index"
    );
    assert_eq!(
        carmignac["official_document_date"],
        "Published 2026-10-13; snapshot data 2026-09-30; accessed 2026-10-17"
    );
    assert!(carmignac["official_provenance"]
        .as_str()
        .unwrap()
        .contains("rebalance conventions and feed availability unverified"));
    let hamco = candidates
        .iter()
        .find(|candidate| candidate["identifier"] == "LU3038481936")
        .unwrap();
    assert_eq!(hamco["official_comparator"], serde_json::Value::Null);
    assert!(hamco["official_provenance"]
        .as_str()
        .unwrap()
        .contains("Official benchmark: none"));
    let dnca = candidates
        .iter()
        .find(|candidate| candidate["identifier"] == "LU1694789535")
        .unwrap();
    assert_eq!(dnca["objective_comparator"], "€STR + 1.20%");
    assert_eq!(
        dnca["objective_document_date"],
        "September 2026 report; data as of 2026-09-30; accessed 2026-10-17"
    );
}

#[tokio::test]
async fn public_dispatch_assignment_replace_remove_and_list_preserve_accounting_and_cache() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKE1", "Closed fixture", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-01", 1.0, 10.0, 0.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-01-02", 1.0, 11.0, 0.0).await;
    let closed_unassigned_id =
        common::insert_asset(&db, "XFAKE2", "Closed unassigned", "stock", "EUR").await;
    common::insert_transaction(&db, closed_unassigned_id, "2025-01-01", 1.0, 10.0, 0.0).await;
    common::insert_sell_transaction(&db, closed_unassigned_id, "2025-01-02", 1.0, 11.0, 0.0).await;
    common::insert_portfolio_snapshot(&db, "2025-01-02", 10.0, 1.0).await;
    common::insert_portfolio_asset_snapshot(&db, "2025-01-02", asset_id, 1.0, 10.0, 10.0, 1.0)
        .await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-01-02", 0.91).await;
    daily_asset_price::Entity::insert(daily_asset_price::ActiveModel {
        asset_id: sea_orm::Set(asset_id),
        date: sea_orm::Set("2025-01-02".to_owned()),
        closing_price: sea_orm::Set(10.0),
        is_api_failure: sea_orm::Set(false),
        ..Default::default()
    })
    .exec(&db)
    .await
    .unwrap();
    benchmark_reference_price::Entity::insert(benchmark_reference_price::ActiveModel {
        reference_id: sea_orm::Set("legacy-acwi-etf-proxy".to_owned()),
        date: sea_orm::Set("2025-01-02".to_owned()),
        closing_price: sea_orm::Set(99.5),
        is_api_failure: sea_orm::Set(false),
        ..Default::default()
    })
    .exec(&db)
    .await
    .unwrap();
    let portfolio_before_assignments = portfolio_state(&db).await;

    let mut out = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign",
            "--ticker",
            "XFAKE1",
            "msci-world-net-eur-unhedged",
            "--override-selection",
        ]),
        OutputFormat::Json,
        &mut out,
    )
    .await
    .unwrap();
    let assigned = common::parse_json_envelope(&out);
    assert_eq!(
        assigned["data"]["definition"]["id"],
        "msci-world-net-eur-unhedged"
    );
    assert_eq!(assigned["data"]["retrospective"], true);
    assert_eq!(
        assigned["data"]["definition"]["verification_state"],
        "target_only"
    );

    let mut candidate_out = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-candidate",
            "--identifier",
            "LU1598719752",
            "--vehicle",
            "fund",
            "msci-acwi-small-cap-value-net-eur",
        ]),
        OutputFormat::Json,
        &mut candidate_out,
    )
    .await
    .unwrap();
    assert_eq!(
        common::parse_json_envelope(&candidate_out)["data"]["creates_holding"],
        false
    );
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-candidate",
            "--identifier",
            "LU1598719752",
            "--vehicle",
            "fund",
            "msci-world-net-eur-unhedged",
        ]),
        OutputFormat::Json,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    let replacement =
        benchmark_assignment_repo::find_by_subject(&db, "fund_candidate", "fund:LU1598719752")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(replacement.reference_id, "msci-world-net-eur-unhedged");
    assert!(!replacement.is_override);
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign-candidate",
            "--identifier",
            "ES0119199018",
            "--vehicle",
            "fund",
            "msci-acwi-small-cap-value-net-eur",
        ]),
        OutputFormat::Json,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    assert_eq!(
        benchmark_assignment_repo::find_by_subject(&db, "fund_candidate", "fund:LU1598719752")
            .await
            .unwrap()
            .unwrap()
            .reference_id,
        "msci-world-net-eur-unhedged"
    );
    assert_eq!(
        benchmark_assignment_repo::find_by_subject(&db, "fund_candidate", "fund:ES0119199018")
            .await
            .unwrap()
            .unwrap()
            .reference_id,
        "msci-acwi-small-cap-value-net-eur"
    );

    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "assign",
            "--ticker",
            "XFAKE1",
            "msci-europe-small-cap-net-eur",
        ]),
        OutputFormat::Human,
        &mut out,
    )
    .await
    .unwrap();
    assert!(String::from_utf8(out.clone())
        .unwrap()
        .contains("MSCI Europe Small Cap"));
    let assigned = benchmark_assignment_repo::find_by_subject(&db, "tracked", "XFAKE1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(assigned.reference_id, "msci-europe-small-cap-net-eur");
    assert!(!assigned.is_override);

    let mut list_json = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&["rstock", "benchmark", "list"]),
        OutputFormat::Json,
        &mut list_json,
    )
    .await
    .unwrap();
    let list = common::parse_json_envelope(&list_json);
    assert_eq!(
        list["data"]["unassigned_tracked_assets"][0]["ticker"],
        "XFAKE2"
    );
    assert_eq!(
        list["data"]["unassigned_tracked_assets"][0]["has_history"],
        true
    );
    assert_eq!(list["data"]["assignments"].as_array().unwrap().len(), 3);
    let candidate_assignment = list["data"]["assignments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["subject_id"] == "fund:LU1598719752")
        .unwrap();
    assert_eq!(
        candidate_assignment["reference_id"],
        "msci-world-net-eur-unhedged"
    );
    assert_eq!(candidate_assignment["retrospective"], true);
    assert_eq!(candidate_assignment["creates_holding"], false);
    assert!(list["data"]["assignment_semantics"]
        .as_str()
        .unwrap()
        .contains("retrospectively"));
    let mut human_list = Vec::new();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&["rstock", "benchmark", "list"]),
        OutputFormat::Human,
        &mut human_list,
    )
    .await
    .unwrap();
    let human_list = String::from_utf8(human_list).unwrap();
    assert!(human_list.contains("retrospective: true; creates holding: false"));
    assert!(human_list.contains(list["data"]["assignment_semantics"].as_str().unwrap()));

    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&[
            "rstock",
            "benchmark",
            "remove-candidate",
            "--identifier",
            "LU1598719752",
            "--vehicle",
            "fund",
        ]),
        OutputFormat::Json,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    rstock::cli::run_benchmark_command_with_writer(
        &db,
        benchmark_command(&["rstock", "benchmark", "remove", "--ticker", "XFAKE1"]),
        OutputFormat::Json,
        &mut Vec::new(),
    )
    .await
    .unwrap();
    assert!(
        benchmark_assignment_repo::find_by_subject(&db, "tracked", "XFAKE1")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        benchmark_assignment_repo::find_by_subject(&db, "fund_candidate", "fund:LU1598719752")
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        benchmark_assignment_repo::find_by_subject(&db, "fund_candidate", "fund:ES0119199018")
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(portfolio_state(&db).await, portfolio_before_assignments);
}

#[tokio::test]
async fn catalog_migrations_up_down_up_preserve_populated_portfolio_and_legacy_reference_cache() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKE2", "Migration fixture", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-01", 2.0, 9.0, 0.0).await;
    common::insert_portfolio_snapshot(&db, "2025-01-02", 18.0, 1.0).await;
    common::insert_portfolio_asset_snapshot(&db, "2025-01-02", asset_id, 2.0, 9.0, 18.0, 1.0).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-01-02", 0.9).await;
    daily_asset_price::Entity::insert(daily_asset_price::ActiveModel {
        asset_id: sea_orm::Set(asset_id),
        date: sea_orm::Set("2025-01-02".to_owned()),
        closing_price: sea_orm::Set(9.0),
        is_api_failure: sea_orm::Set(false),
        ..Default::default()
    })
    .exec(&db)
    .await
    .unwrap();
    benchmark_reference_price::Entity::insert(benchmark_reference_price::ActiveModel {
        reference_id: sea_orm::Set("legacy-acwi-etf-proxy".to_owned()),
        date: sea_orm::Set("2025-01-02".to_owned()),
        closing_price: sea_orm::Set(88.0),
        is_api_failure: sea_orm::Set(false),
        ..Default::default()
    })
    .exec(&db)
    .await
    .unwrap();
    let initial_portfolio_state = portfolio_state(&db).await;

    rollback_benchmark_migration_boundary(&db).await;
    assert_eq!(portfolio_state(&db).await, initial_portfolio_state);
    assert_eq!(
        portfolio_history::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        daily_exchange_rate::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        benchmark_reference_price::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    Migrator::up(&db, Some(BENCHMARK_MIGRATION_BOUNDARY))
        .await
        .unwrap();
    assert!(Migrator::get_pending_migrations(&db)
        .await
        .unwrap()
        .is_empty(),);
    assert_eq!(
        benchmark_candidate::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        12
    );
    assert_eq!(
        benchmark_reference_repo::find_by_id(&db, "legacy-acwi-etf-proxy")
            .await
            .unwrap()
            .source_identifier,
        "ACWI"
    );
    assert_eq!(
        benchmark_reference_price::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(transaction::Entity::find().all(&db).await.unwrap().len(), 1);
    assert_eq!(
        portfolio_history::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        daily_exchange_rate::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        benchmark_reference::Entity::find()
            .all(&db)
            .await
            .unwrap()
            .len(),
        14
    );
    assert_eq!(portfolio_state(&db).await, initial_portfolio_state);
}

#[derive(Debug, PartialEq)]
struct PortfolioState {
    assets: Vec<asset::Model>,
    transactions: Vec<transaction::Model>,
    daily_asset_prices: Vec<daily_asset_price::Model>,
    exchange_rates: Vec<daily_exchange_rate::Model>,
    portfolio_history: Vec<portfolio_history::Model>,
    portfolio_asset_history: Vec<portfolio_asset_history::Model>,
    benchmark_reference_prices: Vec<benchmark_reference_price::Model>,
}

async fn portfolio_state(db: &sea_orm::DatabaseConnection) -> PortfolioState {
    PortfolioState {
        assets: asset::Entity::find()
            .order_by_asc(asset::Column::Id)
            .all(db)
            .await
            .unwrap(),
        transactions: transaction::Entity::find()
            .order_by_asc(transaction::Column::Id)
            .all(db)
            .await
            .unwrap(),
        daily_asset_prices: daily_asset_price::Entity::find()
            .order_by_asc(daily_asset_price::Column::Id)
            .all(db)
            .await
            .unwrap(),
        exchange_rates: daily_exchange_rate::Entity::find()
            .order_by_asc(daily_exchange_rate::Column::Id)
            .all(db)
            .await
            .unwrap(),
        portfolio_history: portfolio_history::Entity::find()
            .order_by_asc(portfolio_history::Column::Date)
            .all(db)
            .await
            .unwrap(),
        portfolio_asset_history: portfolio_asset_history::Entity::find()
            .order_by_asc(portfolio_asset_history::Column::Id)
            .all(db)
            .await
            .unwrap(),
        benchmark_reference_prices: benchmark_reference_price::Entity::find()
            .order_by_asc(benchmark_reference_price::Column::Id)
            .all(db)
            .await
            .unwrap(),
    }
}

async fn rollback_benchmark_migration_boundary(db: &sea_orm::DatabaseConnection) {
    Migrator::down(db, Some(BENCHMARK_MIGRATION_BOUNDARY))
        .await
        .expect("catalog and exact-candidate migrations should roll back");
}

fn command(args: &[&str]) -> rstock::cli::Commands {
    Cli::try_parse_from(args).unwrap().command
}

fn benchmark_command(args: &[&str]) -> rstock::cli::BenchmarkCommands {
    match command(args) {
        rstock::cli::Commands::Benchmark(arguments) => arguments.command,
        other => panic!("expected benchmark command, got {other:?}"),
    }
}
