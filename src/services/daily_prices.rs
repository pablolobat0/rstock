use std::collections::HashMap;

use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, is_benchmark_ticker, DATE_FORMAT};
use crate::db::repos::daily_price_repo;
use crate::models::{Asset, AssetType};
use crate::services::price::PriceFetcher;

pub async fn get_closing_price(
    db: &DatabaseConnection,
    asset: &Asset,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    // Check cache first (non-failure entry)
    if let Some(price) = daily_price_repo::find_price(db, asset.id, date).await? {
        return Ok(Some(price));
    }

    // Forward-fill: find most recent price on or before this date
    daily_price_repo::find_price_at_or_before(db, asset.id, date).await
}

/// Fetches the latest price from the API without persisting to DB.
/// Returns the most recent price returned by the API, or `None` if unavailable.
pub async fn fetch_live_price(
    asset: &Asset,
    date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<f64>> {
    let lookup = match asset.asset_type {
        AssetType::Stock => asset.ticker.as_str(),
        AssetType::Fund | AssetType::Etf => {
            let Some(code) = asset.morningstar_code.as_deref() else {
                tracing::warn!(
                    ticker = %asset.ticker,
                    "skipping price fetch: fund/ETF has no morningstar_code set"
                );
                return Ok(None);
            };
            code
        }
    };
    let prices = price_fetcher
        .get_historical_prices(lookup, date, date, &asset.asset_type)
        .await?;
    Ok(prices.last().map(|(_, price)| *price))
}

/// Fetches live prices for all stock assets (excluding benchmarks) in parallel.
/// Returns a map of `asset_id` -> price for assets where the fetch succeeded.
pub async fn fetch_live_prices_batch(
    assets: &[Asset],
    today: &str,
    price_fetcher: &dyn PriceFetcher,
) -> HashMap<i32, f64> {
    let stock_assets: Vec<_> = assets
        .iter()
        .filter(|a| a.asset_type == AssetType::Stock && !is_benchmark_ticker(&a.ticker))
        .collect();
    let futures: Vec<_> = stock_assets
        .iter()
        .map(|asset| async {
            let result = fetch_live_price(asset, today, price_fetcher).await;
            (asset.id, result)
        })
        .collect();
    futures::future::join_all(futures)
        .await
        .into_iter()
        .filter_map(|(id, r)| r.ok().flatten().map(|p| (id, p)))
        .collect()
}

/// Returns the most recent price and its date on or before the given date.
pub async fn get_price_and_date_at_or_before(
    db: &DatabaseConnection,
    asset_id: i32,
    date: &str,
) -> anyhow::Result<Option<(f64, String)>> {
    daily_price_repo::find_price_and_date_at_or_before(db, asset_id, date).await
}

/// Fetches historical prices from the API and caches them in `daily_asset_prices`.
/// Forward-fills only between API data points (weekends/holidays), never beyond the last API date.
/// Returns the latest date for which the API returned a price, or `None` if the API failed/returned empty.
pub async fn fill_prices_for_range(
    db: &DatabaseConnection,
    asset: &Asset,
    lookup_identifier: &str,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<String>> {
    let prices = price_fetcher
        .get_historical_prices(lookup_identifier, start_date, end_date, &asset.asset_type)
        .await;

    let requested_end =
        NaiveDate::parse_from_str(end_date, DATE_FORMAT).context("invalid end date")?;
    let latest_completed_date = chrono::Local::now().date_naive() - chrono::Duration::days(1);

    let price_map: std::collections::HashMap<String, f64> = match prices {
        Ok(prices) => prices
            .into_iter()
            .filter(|(date, _)| {
                NaiveDate::parse_from_str(date, DATE_FORMAT)
                    .is_ok_and(|date| date <= requested_end && date <= latest_completed_date)
            })
            .collect(),
        Err(e) => {
            tracing::warn!(ticker = %asset.ticker, error = %e, "failed to fetch historical prices");
            return Ok(None);
        }
    };

    let last_api_date = match price_map.keys().max() {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    // Only fill up to the last date the API returned data for
    let start = NaiveDate::parse_from_str(start_date, DATE_FORMAT).context("invalid start date")?;
    let fill_end =
        NaiveDate::parse_from_str(&last_api_date, DATE_FORMAT).context("invalid last API date")?;

    let mut last_known_price =
        daily_price_repo::find_price_before(db, asset.id, start_date).await?;

    let mut current = start;
    while current <= fill_end {
        let date_str = format_date(current);

        if let Some(&api_price) = price_map.get(&date_str) {
            daily_price_repo::upsert(db, asset.id, &date_str, api_price, false).await?;
            last_known_price = Some(api_price);
        } else if let Some(fill_price) = last_known_price {
            // Weekend/holiday between trading days — forward-fill with last known price
            if !daily_price_repo::exists(db, asset.id, &date_str).await? {
                daily_price_repo::upsert(db, asset.id, &date_str, fill_price, false).await?;
            }
        }

        current += chrono::Duration::days(1);
    }

    Ok(Some(last_api_date))
}
