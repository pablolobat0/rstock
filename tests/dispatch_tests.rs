//! Library-owned command dispatch seam tests.
//!
//! `rstock::cli::run_command` is the one command-dispatch interface shared by
//! the executable bootstrap, integration tests, and benchmarks. These tests
//! drive it directly with an in-memory database and fake market sources;
//! process-level wiring and dashboard aliases are covered by the CLI process
//! tests.

pub mod common;

use chrono::NaiveDate;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter};

use common::*;
use rstock::cli::output::OutputFormat;
use rstock::cli::{
    run_command, AssetArgs, AssetCommands, ChartPeriod, Commands, PortfolioArgs, PortfolioCommands,
    TransactionArgs, TransactionCommands,
};
use rstock::db::entities::{asset, transaction};
use rstock::models::{AssetClass, AssetType};
use rstock::services::market_data::MarketData;

async fn dispatch_command(
    db: &DatabaseConnection,
    market_data: &MarketData,
    command: Commands,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    run_command(db, market_data, command, output_format).await
}

#[tokio::test]
async fn get_alias_dispatches_through_the_library() {
    let db = setup_test_db().await;
    let sources = MockMarketDataSources::new();
    let market_data = market_data(&sources);

    dispatch_command(
        &db,
        &market_data,
        Commands::Get {
            period: ChartPeriod::OneYear,
        },
        OutputFormat::Human,
    )
    .await
    .expect("get should dispatch through the library");
}

#[tokio::test]
async fn portfolio_get_alias_dispatches_through_the_library() {
    let db = setup_test_db().await;
    let sources = MockMarketDataSources::new();
    let market_data = market_data(&sources);

    dispatch_command(
        &db,
        &market_data,
        Commands::Portfolio(PortfolioArgs {
            command: PortfolioCommands::Get {
                period: ChartPeriod::All,
            },
        }),
        OutputFormat::Json,
    )
    .await
    .expect("portfolio get should dispatch through the library");
}

#[tokio::test]
async fn dispatch_routes_asset_and_transaction_mutations() {
    let db = setup_test_db().await;
    let sources = MockMarketDataSources::new();
    let market_data = market_data(&sources);

    dispatch_command(
        &db,
        &market_data,
        Commands::Portfolio(PortfolioArgs {
            command: PortfolioCommands::Asset(AssetArgs {
                command: AssetCommands::Add {
                    ticker: "XFAKE1".to_owned(),
                    name: "Fake asset".to_owned(),
                    asset_type: AssetType::Stock,
                    currency: "EUR".to_owned(),
                    asset_class: AssetClass::Equity,
                    equity_style: None,
                    bond_credit: None,
                    bond_duration: None,
                    management: None,
                    morningstar_code: None,
                },
            }),
        }),
        OutputFormat::Json,
    )
    .await
    .expect("portfolio asset add should dispatch through the library");

    let asset_record = asset::Entity::find()
        .filter(asset::Column::Ticker.eq("XFAKE1"))
        .one(&db)
        .await
        .expect("asset lookup should succeed")
        .expect("dispatched asset add should persist the asset");

    dispatch_command(
        &db,
        &market_data,
        Commands::Transaction(TransactionArgs {
            command: TransactionCommands::Buy {
                ticker: "XFAKE1".to_owned(),
                date: NaiveDate::from_ymd_opt(2025, 1, 1).expect("fixed date"),
                quantity: 2.0,
                price: 100.0,
                fees: 0.0,
            },
        }),
        OutputFormat::Json,
    )
    .await
    .expect("transaction buy should dispatch through the library");

    let transactions = transaction::Entity::find()
        .filter(transaction::Column::AssetId.eq(asset_record.id))
        .all(&db)
        .await
        .expect("transaction lookup should succeed");
    assert_eq!(transactions.len(), 1);
    assert_eq!(transactions[0].tx_type, "buy");
}

#[tokio::test]
async fn dispatch_routes_transaction_list() {
    let db = setup_test_db().await;
    let sources = MockMarketDataSources::new();
    let market_data = market_data(&sources);

    let asset_id = insert_asset(&db, "XFAKE2", "Fake asset two", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 1.0, 100.0, 0.0).await;

    dispatch_command(
        &db,
        &market_data,
        Commands::Transaction(TransactionArgs {
            command: TransactionCommands::List {},
        }),
        OutputFormat::Human,
    )
    .await
    .expect("transaction list should dispatch through the library");
}

#[tokio::test]
async fn dispatch_propagates_command_errors() {
    let db = setup_test_db().await;
    let sources = MockMarketDataSources::new();
    let market_data = market_data(&sources);

    let result = dispatch_command(
        &db,
        &market_data,
        Commands::Transaction(TransactionArgs {
            command: TransactionCommands::Delete { id: 999, yes: true },
        }),
        OutputFormat::Json,
    )
    .await;

    let error = result.expect_err("deleting a missing transaction must fail");
    assert!(error.to_string().contains("not found"));
}
