use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::db::repos::exchange_rate_repo;
use crate::services::exchange_rates;
use crate::services::price::PriceFetcher;

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
                tracing::warn!(%pair, "no new exchange rate data from API, falling back to latest cached date");
                if let Some(cached) = exchange_rate_repo::find_latest_date(db, pair).await? {
                    latest_dates.insert(pair.clone(), cached);
                } else {
                    tracing::warn!(%pair, "no cached exchange rate data available at all");
                }
            }
            Err(e) => {
                tracing::warn!(%pair, error = %e, "failed to fill exchange rates, falling back to latest cached date");
                if let Some(cached) = exchange_rate_repo::find_latest_date(db, pair).await? {
                    latest_dates.insert(pair.clone(), cached);
                }
            }
        }
    }
    Ok(latest_dates)
}
