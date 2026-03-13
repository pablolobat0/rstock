mod cli;
mod db;
mod display;
mod models;
mod services;

use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use clap::Parser;
use cli::{ChartPeriod, Cli, Commands};
use models::{AssetInfo, BuyOrder};
use services::price::RealPriceFetcher;

use crate::db::repos::portfolio_history_repo;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let db = db::connect().await?;
    let fetcher = RealPriceFetcher;

    match cli.command {
        Commands::Get { period } => {
            let summary = services::portfolio::get_portfolio_summary(&db, &fetcher).await?;
            let result = services::portfolio::get_portfolio(&db).await?;

            display::print_portfolio(&result, summary.as_ref());

            // NAV chart
            let today = chrono::Local::now().date_naive();
            let today_str = today.format("%Y-%m-%d").to_string();

            let (start_date, period_label) = match period {
                ChartPeriod::Ytd => {
                    let d = NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap();
                    (d, "YTD")
                }
                ChartPeriod::OneYear => {
                    (today - chrono::Duration::days(365), "1Y")
                }
                ChartPeriod::ThreeYears => {
                    (today - chrono::Duration::days(1095), "3Y")
                }
                ChartPeriod::FiveYears => {
                    (today - chrono::Duration::days(1825), "5Y")
                }
                ChartPeriod::All => {
                    let earliest = portfolio_history_repo::find_earliest(&db).await?;
                    match earliest {
                        Some(s) => {
                            let d = NaiveDate::parse_from_str(&s.date, "%Y-%m-%d")
                                .context("invalid inception date")?;
                            (d, "All")
                        }
                        None => (today, "All"),
                    }
                }
            };

            let start_str = start_date.format("%Y-%m-%d").to_string();
            let snapshots = portfolio_history_repo::find_between(&db, &start_str, &today_str).await?;
            display::print_nav_chart(&snapshots, period_label);
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
            let today = chrono::Local::now().date_naive();
            if date > today {
                anyhow::bail!("Date cannot be in the future: {}", date);
            }

            let asset = AssetInfo {
                ticker,
                name,
                asset_type,
                isin,
                currency,
            };
            let order = BuyOrder {
                date: date.format("%Y-%m-%d").to_string(),
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
