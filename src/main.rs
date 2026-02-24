mod cli;
mod db;
mod error;
mod models;
mod services;
mod settings;

use clap::Parser;
use cli::{Cli, Commands};
use models::{AssetInfo, BuyOrder};
use tabled::Table;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let db = db::connect().await?;

    match cli.command {
        Commands::Get => {
            let rows = services::portfolio::get_portfolio(&db).await?;
            if rows.is_empty() {
                println!("No positions found.");
            } else {
                println!("{}", Table::new(&rows));
            }

            if let Some(summary) = services::portfolio::get_portfolio_summary(&db).await? {
                println!();
                println!("Portfolio Value: {:.2}", summary.total_value);
                println!("NAV:            {:.2}", summary.nav);

                let fmt_ret = |r: Option<f64>| match r {
                    Some(v) => {
                        let sign = if v >= 0.0 { "+" } else { "" };
                        format!("{}{:.2}%", sign, v)
                    }
                    None => "N/A".to_string(),
                };

                println!(
                    "YTD: {}  1Y: {}  3Y: {}  5Y: {}",
                    fmt_ret(summary.ytd_return),
                    fmt_ret(summary.one_year_return),
                    fmt_ret(summary.three_year_return),
                    fmt_ret(summary.five_year_return),
                );
            }
        }
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
