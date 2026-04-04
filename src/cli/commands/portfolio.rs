use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, DATE_FORMAT, FIVE_YEAR_DAYS, ONE_MONTH_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS,
    THREE_MONTH_DAYS, THREE_YEAR_DAYS,
};
use crate::services;
use crate::services::price::PriceFetcher;

use super::super::display;
use super::super::ChartPeriod;

pub async fn get(
    db: &DatabaseConnection,
    fetcher: &dyn PriceFetcher,
    period: ChartPeriod,
) -> anyhow::Result<()> {
    let summary = services::portfolio::get_portfolio_summary(db, fetcher).await?;
    let result = services::portfolio::get_portfolio(db, fetcher).await?;

    display::print_portfolio(&result, summary.as_ref());

    let today = chrono::Local::now().date_naive();
    let today_str = format_date(today);

    let (start_date, period_label) = match period {
        ChartPeriod::OneMonth => (today - chrono::Duration::days(ONE_MONTH_DAYS), "1M"),
        ChartPeriod::ThreeMonths => (today - chrono::Duration::days(THREE_MONTH_DAYS), "3M"),
        ChartPeriod::SixMonths => (today - chrono::Duration::days(SIX_MONTH_DAYS), "6M"),
        ChartPeriod::Ytd => {
            let d = NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 is always valid");
            (d, "YTD")
        }
        ChartPeriod::OneYear => (today - chrono::Duration::days(ONE_YEAR_DAYS), "1Y"),
        ChartPeriod::ThreeYears => (today - chrono::Duration::days(THREE_YEAR_DAYS), "3Y"),
        ChartPeriod::FiveYears => (today - chrono::Duration::days(FIVE_YEAR_DAYS), "5Y"),
        ChartPeriod::All => {
            let inception = services::portfolio::get_inception_date(db).await?;
            match inception {
                Some(date_str) => {
                    let d = NaiveDate::parse_from_str(&date_str, DATE_FORMAT)
                        .context("invalid inception date")?;
                    (d, "All")
                }
                None => (today, "All"),
            }
        }
    };

    let start_str = format_date(start_date);
    let snapshots = services::portfolio::get_nav_snapshots(db, &start_str, &today_str).await?;
    display::print_nav_chart(&snapshots, period_label);

    Ok(())
}

pub async fn list(db: &DatabaseConnection) -> anyhow::Result<()> {
    let assets = services::portfolio::list_assets(db).await?;
    display::print_asset_list(&assets);
    Ok(())
}

pub async fn holdings(db: &DatabaseConnection, fetcher: &dyn PriceFetcher) -> anyhow::Result<()> {
    let result = services::holdings::get_holdings(db, fetcher).await?;
    display::print_holdings(&result);
    Ok(())
}
