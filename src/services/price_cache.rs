use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::models::Asset;
use crate::services::daily_prices;
use crate::services::exchange_rates;
use crate::services::price::PriceFetcher;

/// Fills price caches for all assets in parallel and returns a map of `asset_id` → latest asset price date.
pub async fn fill_asset_prices(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<HashMap<i32, String>> {
    tracing::debug!(asset_count = assets.len(), %start_date, %end_date, "filling asset price cache");

    let futures: Vec<_> = assets
        .iter()
        .map(|asset| async move {
            let result =
                daily_prices::fill_prices_for_range(db, asset, start_date, end_date, price_fetcher)
                    .await;
            (asset, result)
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
                tracing::warn!(ticker = %asset.ticker, "no price data available");
            }
            Err(e) => {
                tracing::warn!(ticker = %asset.ticker, error = %e, "failed to fill prices");
            }
        }
    }
    Ok(latest_dates)
}

/// Fills exchange rate caches for all needed currency pairs in parallel.
/// Returns a map of pair → latest API date.
pub async fn fill_exchange_rates(
    db: &DatabaseConnection,
    pairs: &[String],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<HashMap<String, String>> {
    tracing::debug!(pair_count = pairs.len(), %start_date, %end_date, "filling exchange rate cache");

    let futures: Vec<_> = pairs
        .iter()
        .map(|pair| async move {
            let result =
                exchange_rates::fill_rates_for_range(db, pair, start_date, end_date, price_fetcher)
                    .await;
            (pair, result)
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let mut latest_dates: HashMap<String, String> = HashMap::new();
    for (pair, result) in results {
        match result {
            Ok(Some(date)) => {
                latest_dates.insert(pair.clone(), date);
            }
            Ok(None) => {
                tracing::warn!(%pair, "no exchange rate data available");
            }
            Err(e) => {
                tracing::warn!(%pair, error = %e, "failed to fill exchange rates");
            }
        }
    }
    Ok(latest_dates)
}
