use std::collections::HashMap;

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::DATE_FORMAT;
use crate::models::Asset;
use crate::services::price::PriceFetcher;
use crate::services::price_cache;

pub async fn prepare_nav_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    currency_pairs: &[String],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<NaiveDate> {
    let latest_asset_dates =
        price_cache::fill_asset_prices(db, assets, start_date, end_date, price_fetcher).await?;
    let latest_rate_dates = if currency_pairs.is_empty() {
        HashMap::new()
    } else {
        price_cache::fill_exchange_rates(db, currency_pairs, start_date, end_date, price_fetcher)
            .await?
    };

    let mut latest_required_dates = Vec::with_capacity(assets.len() + currency_pairs.len() + 1);
    latest_required_dates.push(parse_date(end_date, "NAV end date")?);

    for asset in assets {
        let Some(latest_date) = latest_asset_dates.get(&asset.id) else {
            bail!(
                "missing required historical market data for asset {} ({})",
                asset.ticker,
                asset.name
            );
        };
        latest_required_dates.push(parse_date(latest_date, "asset price date")?);
    }

    for pair in currency_pairs {
        let Some(latest_date) = latest_rate_dates.get(pair) else {
            bail!("missing required historical market data for FX rate {pair}");
        };
        latest_required_dates.push(parse_date(latest_date, "FX rate date")?);
    }

    latest_required_dates
        .into_iter()
        .min()
        .context("NAV market data preparation had no date requirements")
}

fn parse_date(date: &str, label: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).with_context(|| format!("invalid {label}: {date}"))
}
