mod cli;
mod constants;
mod db;
mod display;
mod models;
mod services;
mod utils;

use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use clap::Parser;
use cli::{ChartPeriod, Cli, Commands};
use constants::{format_date, DATE_FORMAT, FIVE_YEAR_DAYS, ONE_YEAR_DAYS, THREE_YEAR_DAYS};
use models::{AssetInfo, BuyOrder, DividendOrder, SellOrder, SplitOrder};
use services::price::RealPriceFetcher;

use crate::db::repos::{asset_repo, portfolio_history_repo};

#[tokio::main]
#[allow(clippy::too_many_lines)]
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
            let today_str = format_date(today);

            let (start_date, period_label) = match period {
                ChartPeriod::Ytd => {
                    let d =
                        NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 is always valid");
                    (d, "YTD")
                }
                ChartPeriod::OneYear => (today - chrono::Duration::days(ONE_YEAR_DAYS), "1Y"),
                ChartPeriod::ThreeYears => (today - chrono::Duration::days(THREE_YEAR_DAYS), "3Y"),
                ChartPeriod::FiveYears => (today - chrono::Duration::days(FIVE_YEAR_DAYS), "5Y"),
                ChartPeriod::All => {
                    let earliest = portfolio_history_repo::find_earliest(&db).await?;
                    match earliest {
                        Some(s) => {
                            let d = NaiveDate::parse_from_str(&s.date, DATE_FORMAT)
                                .context("invalid inception date")?;
                            (d, "All")
                        }
                        None => (today, "All"),
                    }
                }
            };

            let start_str = format_date(start_date);
            let snapshots =
                portfolio_history_repo::find_between(&db, &start_str, &today_str).await?;
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
                anyhow::bail!("Date cannot be in the future: {date}");
            }

            let asset = AssetInfo {
                ticker,
                name,
                asset_type,
                isin,
                currency,
            };
            let order = BuyOrder {
                date: format_date(date),
                quantity,
                price,
                fees,
                notes,
            };
            services::transactions::buy(&db, asset, order).await?;
        }
        Commands::Dividend {
            ticker,
            date,
            amount,
            fees,
            notes,
        } => {
            let today = chrono::Local::now().date_naive();
            if date > today {
                anyhow::bail!("Date cannot be in the future: {date}");
            }

            let order = DividendOrder {
                date: format_date(date),
                amount,
                fees,
                notes,
            };
            services::transactions::dividend(&db, ticker, order).await?;
        }
        Commands::Holdings {} => {
            let result = services::holdings::get_holdings(&db).await?;
            display::print_holdings(&result);
        }
        Commands::List {} => {
            let assets = asset_repo::find_all(&db).await?;
            display::print_asset_list(&assets);
        }
        Commands::Export { output } => {
            let count = services::export::export_transactions_csv(&db, &output).await?;
            println!("Exported {count} transactions to {output}");
        }
        Commands::Split {
            ticker,
            date,
            ratio,
            notes,
        } => {
            let today = chrono::Local::now().date_naive();
            if date > today {
                anyhow::bail!("Date cannot be in the future: {date}");
            }

            let order = SplitOrder {
                date: format_date(date),
                ratio,
                notes,
            };
            services::transactions::split(&db, ticker, order).await?;
        }
        Commands::Sell {
            ticker,
            date,
            quantity,
            price,
            fees,
            notes,
        } => {
            let today = chrono::Local::now().date_naive();
            if date > today {
                anyhow::bail!("Date cannot be in the future: {date}");
            }

            let order = SellOrder {
                date: format_date(date),
                quantity,
                price,
                fees,
                notes,
            };
            services::transactions::sell(&db, ticker, order).await?;
        }
    }

    Ok(())
}
