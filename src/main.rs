mod cli;
mod db;
mod error;
mod models;
mod services;
mod settings;

use clap::Parser;
use cli::{Cli, Commands};
use models::{AssetInfo, BuyOrder};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let db = db::connect().await?;

    match cli.command {
        Commands::Buy {
            ticker,
            name,
            asset_type,
            isin,
            date,
            quantity,
            price,
            fees,
            currency,
            notes,
        } => {
            let asset = AssetInfo {
                ticker,
                name,
                asset_type: asset_type.to_string(),
                isin,
                currency,
            };
            let order = BuyOrder {
                date,
                quantity,
                price,
                fees,
                notes,
            };
            services::transactions::buy(&db, asset, order).await?;
        }
    }

    Ok(())
}
