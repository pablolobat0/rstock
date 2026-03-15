use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, DATE_FORMAT};
use crate::db::repos::daily_price_repo;
use crate::models::Asset;
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

/// Fetches historical prices from the API and caches them in `daily_asset_prices`.
/// Forward-fills only between API data points (weekends/holidays), never beyond the last API date.
/// Returns the latest date for which the API returned a price, or `None` if the API failed/returned empty.
pub async fn fill_prices_for_range(
    db: &DatabaseConnection,
    asset: &Asset,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<String>> {
    let lookup = asset.isin.as_deref().unwrap_or(&asset.ticker);
    let prices = price_fetcher
        .get_historical_prices(lookup, start_date, end_date, &asset.asset_type)
        .await;

    let price_map: std::collections::HashMap<String, f64> = match prices {
        Ok(prices) => prices.into_iter().collect(),
        Err(e) => {
            eprintln!(
                "Warning: failed to fetch historical prices for {}: {}",
                asset.ticker, e
            );
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
