use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

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

pub async fn fill_prices_for_range(
    db: &DatabaseConnection,
    asset: &Asset,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<()> {
    // Fetch historical prices from API
    let lookup = asset.isin.as_deref().unwrap_or(&asset.ticker);
    let api_prices = price_fetcher
        .get_historical_prices(lookup, start_date, end_date, &asset.asset_type)
        .await;

    let api_failed = api_prices.is_err();
    let price_map: std::collections::HashMap<String, f64> = match api_prices {
        Ok(prices) => prices.into_iter().collect(),
        Err(e) => {
            eprintln!(
                "Warning: failed to fetch historical prices for {}: {}",
                asset.ticker, e
            );
            std::collections::HashMap::new()
        }
    };

    // Iterate calendar days and store prices
    let start =
        NaiveDate::parse_from_str(start_date, "%Y-%m-%d").context("invalid start date")?;
    let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d").context("invalid end date")?;

    // Get the most recent price before start_date for forward-fill
    let mut last_known_price =
        daily_price_repo::find_price_before(db, asset.id, start_date).await?;

    let mut current = start;
    while current <= end {
        let date_str = current.format("%Y-%m-%d").to_string();

        if let Some(&api_price) = price_map.get(&date_str) {
            // We have API data for this day
            daily_price_repo::upsert(db, asset.id, &date_str, api_price, false).await?;
            last_known_price = Some(api_price);
        } else if let Some(fill_price) = last_known_price {
            // No API data (weekend/holiday or API failure) — forward-fill
            let is_failure = api_failed;
            // Only insert if not already cached
            if !daily_price_repo::exists(db, asset.id, &date_str).await? {
                daily_price_repo::upsert(db, asset.id, &date_str, fill_price, is_failure).await?;
            }
        }

        current += chrono::Duration::days(1);
    }

    Ok(())
}
