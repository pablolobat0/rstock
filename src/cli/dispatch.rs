//! Library-owned command dispatch.
//!
//! The executable bootstrap, integration tests, and benchmarks all route
//! parsed CLI commands through [`run_command`] so production and tests always
//! exercise the same application module graph.

use sea_orm::DatabaseConnection;

use super::commands;
use super::output::OutputFormat;
use super::{
    AnalyzeCommands, AssetCommands, Commands, CompareCommands, CorrelationCommands,
    PortfolioCommands, TransactionCommands,
};
use crate::services::market_data::MarketData;

/// Dispatch one parsed CLI command against the application graph.
pub async fn run_command(
    db: &DatabaseConnection,
    market_data: &MarketData,
    command: Commands,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match command {
        Commands::Get { period } => {
            commands::portfolio::get(db, market_data, period, output_format).await
        }
        Commands::Portfolio(args) => match args.command {
            PortfolioCommands::Get { period } => {
                commands::portfolio::get(db, market_data, period, output_format).await
            }
            PortfolioCommands::Asset(asset_args) => {
                run_asset_command(db, asset_args.command, output_format).await
            }
        },
        Commands::Transaction(args) => {
            run_transaction_command(db, args.command, output_format).await
        }
        Commands::Analyze(args) => match args.command {
            AnalyzeCommands::Composition {} => {
                commands::analyze::composition(db, market_data, output_format).await
            }
            AnalyzeCommands::Fund { code, period } => {
                commands::analyze::fund(db, market_data, code, period, output_format).await
            }
            AnalyzeCommands::Correlation(args) => match args.command {
                CorrelationCommands::Matrix { period } => {
                    commands::analyze::correlation_matrix(db, market_data, period, output_format)
                        .await
                }
                CorrelationCommands::Rolling {
                    identifier_a,
                    identifier_b,
                    period,
                } => {
                    commands::analyze::rolling_correlation(
                        db,
                        market_data,
                        identifier_a,
                        identifier_b,
                        period,
                        output_format,
                    )
                    .await
                }
            },
        },
        Commands::Compare(args) => match args.command {
            CompareCommands::Funds {
                code_a,
                code_b,
                period,
            } => {
                commands::compare::funds(db, market_data, code_a, code_b, period, output_format)
                    .await
            }
        },
    }
}

async fn run_transaction_command(
    db: &DatabaseConnection,
    cmd: TransactionCommands,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    match cmd {
        TransactionCommands::List {} => commands::transactions::list(db, output_format).await,
        TransactionCommands::Buy {
            ticker,
            date,
            quantity,
            price,
            fees,
        } => {
            commands::transactions::buy(db, ticker, date, quantity, price, fees, output_format)
                .await
        }
        TransactionCommands::Sell {
            ticker,
            date,
            quantity,
            price,
            fees,
        } => {
            commands::transactions::sell(db, ticker, date, quantity, price, fees, output_format)
                .await
        }
        TransactionCommands::Dividend {
            ticker,
            date,
            amount,
            fees,
        } => commands::transactions::dividend(db, ticker, date, amount, fees, output_format).await,
        TransactionCommands::Split {
            ticker,
            date,
            ratio,
        } => commands::transactions::split(db, ticker, date, ratio, output_format).await,
        TransactionCommands::Edit {
            id,
            date,
            quantity,
            price,
            fees,
            yes,
        } => {
            let options = commands::transactions::EditOptions {
                date,
                quantity,
                price,
                fees,
                yes,
            };
            commands::transactions::edit(db, id, options, output_format).await
        }
        TransactionCommands::Delete { id, yes } => {
            commands::transactions::delete(db, id, yes, output_format).await
        }
        TransactionCommands::Export { output } => {
            commands::export::run(db, output, output_format).await
        }
        TransactionCommands::Import { input } => {
            commands::import::run(db, input, output_format).await
        }
    }
}

async fn run_asset_command(
    db: &DatabaseConnection,
    cmd: AssetCommands,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
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
            commands::portfolio::asset_add(
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
                output_format,
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
            commands::portfolio::asset_edit(
                db,
                ticker,
                name,
                asset_class,
                equity_style,
                bond_credit,
                bond_duration,
                management,
                morningstar_code,
                output_format,
            )
            .await
        }
    }
}
