use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::*;

use crate::db::entities::{asset, daily_asset_price};
use crate::services::price::PriceFetcher;

pub async fn get_closing_price(
    // Rebuild portfolio history (smart: only fills from last snapshot)
    db: &DatabaseConnection,
    asset: &asset::Model,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    // Check cache first (non-failure entry)
    if let Some(cached) = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset.id))
        .filter(daily_asset_price::Column::Date.eq(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .one(db)
        .await?
    {
        return Ok(Some(cached.closing_price));
    }

    // Forward-fill: find most recent price on or before this date
    if let Some(prev) = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset.id))
        .filter(daily_asset_price::Column::Date.lte(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_desc(daily_asset_price::Column::Date)
        .one(db)
        .await?
    {
        return Ok(Some(prev.closing_price));
    }

    Ok(None)
}

pub async fn fill_prices_for_range(
    db: &DatabaseConnection,
    asset: &asset::Model,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<()> {
    // Fetch historical prices from API (Yahoo Finance for all asset types)
    // Use ISIN for funds/ETFs (Yahoo Finance resolves ISINs), ticker for stocks
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
    let start = NaiveDate::parse_from_str(start_date, "%Y-%m-%d").context("invalid start date")?;
    let end = NaiveDate::parse_from_str(end_date, "%Y-%m-%d").context("invalid end date")?;

    let mut last_known_price: Option<f64> = None;

    // Get the most recent price before start_date for forward-fill
    if let Some(prev) = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset.id))
        .filter(daily_asset_price::Column::Date.lt(start_date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_desc(daily_asset_price::Column::Date)
        .one(db)
        .await?
    {
        last_known_price = Some(prev.closing_price);
    }

    let mut current = start;
    while current <= end {
        let date_str = current.format("%Y-%m-%d").to_string();

        if let Some(&api_price) = price_map.get(&date_str) {
            // We have API data for this day
            upsert_price(db, asset.id, &date_str, api_price, false).await?;
            last_known_price = Some(api_price);
        } else if let Some(fill_price) = last_known_price {
            // No API data (weekend/holiday or API failure) — forward-fill
            let is_failure = api_failed;
            // Only insert if not already cached
            let existing = daily_asset_price::Entity::find()
                .filter(daily_asset_price::Column::AssetId.eq(asset.id))
                .filter(daily_asset_price::Column::Date.eq(&date_str))
                .one(db)
                .await?;
            if existing.is_none() {
                upsert_price(db, asset.id, &date_str, fill_price, is_failure).await?;
            }
        }

        current += chrono::Duration::days(1);
    }

    Ok(())
}

async fn upsert_price(
    db: &DatabaseConnection,
    asset_id: i32,
    date: &str,
    price: f64,
    is_api_failure: bool,
) -> anyhow::Result<()> {
    // Try to find existing record
    let existing = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.eq(date))
        .one(db)
        .await?;

    if let Some(record) = existing {
        // Update existing
        let mut active: daily_asset_price::ActiveModel = record.into();
        active.closing_price = Set(price);
        active.is_api_failure = Set(is_api_failure);
        active.update(db).await?;
    } else {
        // Insert new
        let record = daily_asset_price::ActiveModel {
            asset_id: Set(asset_id),
            date: Set(date.to_owned()),
            closing_price: Set(price),
            is_api_failure: Set(is_api_failure),
            ..Default::default()
        };
        record.insert(db).await?;
    }

    Ok(())
}
