use std::collections::HashMap;

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::DATE_FORMAT;
use crate::db::repos::daily_price_repo;
use crate::models::Asset;
use crate::models::AssetType;
use crate::services::daily_prices;
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
        fill_nav_asset_prices(db, assets, start_date, end_date, price_fetcher).await?;
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

async fn fill_nav_asset_prices(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<HashMap<i32, String>> {
    tracing::debug!(asset_count = assets.len(), %start_date, %end_date, "filling NAV asset price cache");

    let mut requirements = Vec::with_capacity(assets.len());
    for asset in assets {
        requirements.push((asset, lookup_identifier(asset)?));
    }

    let futures: Vec<_> = requirements
        .iter()
        .map(|(asset, lookup_identifier)| async move {
            let result = daily_prices::fill_prices_for_range(
                db,
                asset,
                lookup_identifier,
                start_date,
                end_date,
                price_fetcher,
            )
            .await;
            (*asset, result)
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let mut latest_dates: HashMap<i32, String> = HashMap::new();
    for (asset, result) in results {
        match result {
            Ok(Some(date)) => {
                latest_dates.insert(asset.id, date);
            }
            Ok(None) => {
                tracing::warn!(ticker = %asset.ticker, "no new price data from API, falling back to latest cached date");
                if let Some(cached) = daily_price_repo::find_latest_date(db, asset.id).await? {
                    latest_dates.insert(asset.id, cached);
                } else {
                    tracing::warn!(ticker = %asset.ticker, "no cached price data available at all");
                }
            }
            Err(e) => {
                tracing::warn!(ticker = %asset.ticker, error = %e, "failed to fill prices, falling back to latest cached date");
                if let Some(cached) = daily_price_repo::find_latest_date(db, asset.id).await? {
                    latest_dates.insert(asset.id, cached);
                }
            }
        }
    }

    Ok(latest_dates)
}

fn lookup_identifier(asset: &Asset) -> anyhow::Result<&str> {
    match asset.asset_type {
        AssetType::Stock => Ok(asset.ticker.as_str()),
        AssetType::Fund | AssetType::Etf => asset.morningstar_code.as_deref().with_context(|| {
            format!(
                "missing Morningstar code for required {} {} ({})",
                asset.asset_type, asset.ticker, asset.name
            )
        }),
    }
}

fn parse_date(date: &str, label: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).with_context(|| format!("invalid {label}: {date}"))
}
