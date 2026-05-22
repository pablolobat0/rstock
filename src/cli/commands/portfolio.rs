use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, DATE_FORMAT, FIVE_YEAR_DAYS, ONE_MONTH_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS,
    THREE_MONTH_DAYS, THREE_YEAR_DAYS,
};
use crate::models::{
    AssetClass, AssetClassification, AssetInfo, AssetType, BondCredit, BondDuration, EquityStyle,
    Management,
};
use crate::services;
use crate::services::market_data::MarketData;

use super::super::display;
use super::super::ChartPeriod;

pub async fn get(
    db: &DatabaseConnection,
    market_data: &MarketData,
    period: ChartPeriod,
) -> anyhow::Result<()> {
    let result = services::portfolio::get_portfolio(db, market_data).await?;

    display::print_portfolio(&result);

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

#[allow(clippy::too_many_arguments)]
pub async fn asset_add(
    db: &DatabaseConnection,
    ticker: String,
    name: String,
    asset_type: AssetType,
    currency: String,
    asset_class: AssetClass,
    equity_style: Option<EquityStyle>,
    bond_credit: Option<BondCredit>,
    bond_duration: Option<BondDuration>,
    management: Option<Management>,
    morningstar_code: Option<String>,
) -> anyhow::Result<()> {
    let info = AssetInfo {
        ticker: ticker.clone(),
        name,
        asset_type,
        currency,
    };
    let classification = AssetClassification {
        asset_class: Some(asset_class),
        equity_style,
        bond_credit,
        bond_duration,
        management,
    };
    services::assets::create_tracked_asset(db, &info, &classification, morningstar_code.as_deref())
        .await?;
    println!("Added asset {ticker}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn asset_edit(
    db: &DatabaseConnection,
    ticker: String,
    name: Option<String>,
    asset_class: Option<AssetClass>,
    equity_style: Option<EquityStyle>,
    bond_credit: Option<BondCredit>,
    bond_duration: Option<BondDuration>,
    management: Option<Management>,
    morningstar_code: Option<String>,
) -> anyhow::Result<()> {
    let classification = AssetClassification {
        asset_class,
        equity_style,
        bond_credit,
        bond_duration,
        management,
    };
    if name.is_none() && morningstar_code.is_none() && classification.is_empty() {
        anyhow::bail!("at least one field must be provided");
    }
    services::assets::update_tracked_asset(
        db,
        &ticker,
        &classification,
        name.as_deref(),
        morningstar_code.as_deref(),
    )
    .await?;
    println!("Updated asset {ticker}");
    Ok(())
}
