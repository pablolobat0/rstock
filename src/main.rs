mod cli;
mod constants;
mod db;
mod logging;
mod models;
mod services;
mod utils;

use clap::Parser;

use cli::{
    AnalyzeCommands, AssetCommands, Cli, Commands, DataCommands, PortfolioCommands,
    TransactionCommands,
};
use sea_orm::DatabaseConnection;
use services::price::RealPriceFetcher;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    logging::init(cli.verbose)?;

    tracing::debug!(command = ?cli.command, "starting rstock");

    let db = db::connect().await?;
    let fetcher = RealPriceFetcher;

    match cli.command {
        Commands::Get { period } => cli::commands::portfolio::get(&db, &fetcher, period).await,
        Commands::Buy {
            ticker,
            date,
            quantity,
            price,
            fees,
        } => cli::commands::transactions::buy(&db, ticker, date, quantity, price, fees).await,
        Commands::Portfolio(args) => match args.command {
            PortfolioCommands::Get { period } => {
                cli::commands::portfolio::get(&db, &fetcher, period).await
            }
            PortfolioCommands::List {} => cli::commands::portfolio::list(&db).await,
            PortfolioCommands::Holdings {} => {
                cli::commands::portfolio::holdings(&db, &fetcher).await
            }
            PortfolioCommands::Asset(asset_args) => {
                run_asset_command(&db, asset_args.command).await
            }
        },
        Commands::Transaction(args) => match args.command {
            TransactionCommands::Buy {
                ticker,
                date,
                quantity,
                price,
                fees,
            } => cli::commands::transactions::buy(&db, ticker, date, quantity, price, fees).await,
            TransactionCommands::Sell {
                ticker,
                date,
                quantity,
                price,
                fees,
            } => cli::commands::transactions::sell(&db, ticker, date, quantity, price, fees).await,
            TransactionCommands::Dividend {
                ticker,
                date,
                amount,
                fees,
            } => cli::commands::transactions::dividend(&db, ticker, date, amount, fees).await,
            TransactionCommands::Split {
                ticker,
                date,
                ratio,
            } => cli::commands::transactions::split(&db, ticker, date, ratio).await,
            TransactionCommands::Edit {
                id,
                date,
                quantity,
                price,
                fees,
                yes,
            } => cli::commands::transactions::edit(&db, id, date, quantity, price, fees, yes).await,
            TransactionCommands::Delete { id, yes } => {
                cli::commands::transactions::delete(&db, id, yes).await
            }
        },
        Commands::Data(args) => match args.command {
            DataCommands::Export { output } => cli::commands::export::run(&db, output).await,
            DataCommands::Import { input } => cli::commands::import::run(&db, input).await,
        },
        Commands::Analyze(args) => match args.command {
            AnalyzeCommands::Composition {} => {
                cli::commands::analyze::composition(&db, &fetcher).await
            }
            AnalyzeCommands::Correlation { period } => {
                cli::commands::analyze::correlation(&db, &fetcher, period).await
            }
        },
        Commands::Monitor(args) => cli::commands::monitor::run(&db, &fetcher, args).await,
    }
}

async fn run_asset_command(db: &DatabaseConnection, cmd: AssetCommands) -> anyhow::Result<()> {
    match cmd {
        AssetCommands::Add {
            ticker,
            name,
            asset_type,
            currency,
            asset_class,
            equity_style,
            bond_credit,
            bond_duration,
            management,
            morningstar_code,
        } => {
            cli::commands::portfolio::asset_add(
                db,
                ticker,
                name,
                asset_type,
                currency,
                asset_class,
                equity_style,
                bond_credit,
                bond_duration,
                management,
                morningstar_code,
            )
            .await
        }
        AssetCommands::Edit {
            ticker,
            name,
            asset_class,
            equity_style,
            bond_credit,
            bond_duration,
            management,
            morningstar_code,
        } => {
            cli::commands::portfolio::asset_edit(
                db,
                ticker,
                name,
                asset_class,
                equity_style,
                bond_credit,
                bond_duration,
                management,
                morningstar_code,
            )
            .await
        }
    }
}
