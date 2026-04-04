use chrono::{Datelike, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{
    FIVE_YEAR_DAYS, ONE_MONTH_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS, THREE_MONTH_DAYS,
    THREE_YEAR_DAYS,
};
use crate::services;
use crate::services::price::PriceFetcher;

use super::super::display;
use super::super::{ChartPeriod, MonitorArgs, MonitorCommands};

pub async fn run(
    db: &DatabaseConnection,
    fetcher: &dyn PriceFetcher,
    args: MonitorArgs,
) -> anyhow::Result<()> {
    match args.command {
        MonitorCommands::Add { ticker, sector_etf } => {
            services::watchlist::add(db, &ticker, &sector_etf).await
        }
        MonitorCommands::Remove { ticker } => services::watchlist::remove(db, &ticker).await,
        MonitorCommands::List {} => {
            let items = services::watchlist::list(db).await?;
            display::print_watchlist(&items);
            Ok(())
        }
        MonitorCommands::View { ticker, period } => {
            let item = services::watchlist::get(db, &ticker).await?;

            let today = chrono::Local::now().date_naive();
            let period_days = match period {
                ChartPeriod::OneMonth => ONE_MONTH_DAYS,
                ChartPeriod::ThreeMonths => THREE_MONTH_DAYS,
                ChartPeriod::SixMonths => SIX_MONTH_DAYS,
                ChartPeriod::Ytd => {
                    let jan1 =
                        NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 is always valid");
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
                fetcher,
            )
            .await?;
            display::print_monitor_report(&report);
            Ok(())
        }
    }
}
