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
use cli::{AnalysisTarget, ChartPeriod, Cli, Commands, CorrelationPeriod, MonitorCommands};
use constants::{
    format_date, DATE_FORMAT, FIVE_YEAR_DAYS, ONE_MONTH_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS,
    THIRTY_DAYS, THREE_MONTH_DAYS, THREE_YEAR_DAYS,
};
use models::{AssetInfo, BuyOrder, DividendOrder, SellOrder, SplitOrder};
use services::price::RealPriceFetcher;

use crate::db::repos::{asset_repo, portfolio_history_repo, watchlist_repo};

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
                ChartPeriod::OneMonth => (today - chrono::Duration::days(ONE_MONTH_DAYS), "1M"),
                ChartPeriod::ThreeMonths => {
                    (today - chrono::Duration::days(THREE_MONTH_DAYS), "3M")
                }
                ChartPeriod::SixMonths => (today - chrono::Duration::days(SIX_MONTH_DAYS), "6M"),
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
            };
            services::transactions::buy(&db, asset, order).await?;
        }
        Commands::Dividend {
            ticker,
            date,
            amount,
            fees,
        } => {
            let today = chrono::Local::now().date_naive();
            if date > today {
                anyhow::bail!("Date cannot be in the future: {date}");
            }

            let order = DividendOrder {
                date: format_date(date),
                amount,
                fees,
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
        } => {
            let today = chrono::Local::now().date_naive();
            if date > today {
                anyhow::bail!("Date cannot be in the future: {date}");
            }

            let order = SplitOrder {
                date: format_date(date),
                ratio,
            };
            services::transactions::split(&db, ticker, order).await?;
        }
        Commands::Analyze { target, period } => match target {
            AnalysisTarget::Portfolio => {
                let today = chrono::Local::now().date_naive();
                let today_str = format_date(today);

                let (start_date, period_label) = match period {
                    CorrelationPeriod::ThirtyDays => {
                        (today - chrono::Duration::days(THIRTY_DAYS), "30D")
                    }
                    CorrelationPeriod::SixMonths => {
                        (today - chrono::Duration::days(SIX_MONTH_DAYS), "6M")
                    }
                    CorrelationPeriod::OneYear => {
                        (today - chrono::Duration::days(ONE_YEAR_DAYS), "1Y")
                    }
                    CorrelationPeriod::ThreeYears => {
                        (today - chrono::Duration::days(THREE_YEAR_DAYS), "3Y")
                    }
                    CorrelationPeriod::FiveYears => {
                        (today - chrono::Duration::days(FIVE_YEAR_DAYS), "5Y")
                    }
                };

                let start_str = format_date(start_date);
                let matrix = services::metrics::compute_correlation_matrix(
                    &db, &start_str, &today_str, &fetcher,
                )
                .await?;

                display::print_correlation_matrix(&matrix, period_label);
            }
        },
        Commands::Sell {
            ticker,
            date,
            quantity,
            price,
            fees,
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
            };
            services::transactions::sell(&db, ticker, order).await?;
        }
        Commands::Monitor(args) => match args.command {
            MonitorCommands::Add { ticker, sector_etf } => {
                if watchlist_repo::find_by_ticker(&db, &ticker)
                    .await?
                    .is_some()
                {
                    anyhow::bail!("{ticker} is already in the watchlist");
                }
                watchlist_repo::insert(&db, &ticker, &sector_etf).await?;
                println!("Added {ticker} with sector ETF {sector_etf} to watchlist.");
            }
            MonitorCommands::Remove { ticker } => {
                if watchlist_repo::delete_by_ticker(&db, &ticker).await? {
                    println!("Removed {ticker} from watchlist.");
                } else {
                    anyhow::bail!("{ticker} is not in the watchlist");
                }
            }
            MonitorCommands::List {} => {
                let items = watchlist_repo::find_all(&db).await?;
                display::print_watchlist(&items);
            }
            MonitorCommands::View { ticker, period } => {
                let item = watchlist_repo::find_by_ticker(&db, &ticker)
                    .await?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "{ticker} is not in the watchlist. Add it first with: rstock monitor add --ticker {ticker} --sector-etf <ETF>"
                        )
                    })?;

                let today = chrono::Local::now().date_naive();
                let period_days = match period {
                    ChartPeriod::OneMonth => ONE_MONTH_DAYS,
                    ChartPeriod::ThreeMonths => THREE_MONTH_DAYS,
                    ChartPeriod::SixMonths => SIX_MONTH_DAYS,
                    ChartPeriod::Ytd => {
                        let jan1 = NaiveDate::from_ymd_opt(today.year(), 1, 1)
                            .expect("Jan 1 is always valid");
                        (today - jan1).num_days()
                    }
                    ChartPeriod::OneYear => ONE_YEAR_DAYS,
                    ChartPeriod::ThreeYears => THREE_YEAR_DAYS,
                    ChartPeriod::FiveYears | ChartPeriod::All => FIVE_YEAR_DAYS,
                };

                let report = services::monitor::generate_monitor_report(
                    &ticker,
                    &item.sector_etf_ticker,
                    period_days,
                    period.label(),
                    &fetcher,
                )
                .await?;
                display::print_monitor_report(&report);
            }
        },
    }

    Ok(())
}
