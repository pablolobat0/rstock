use std::collections::HashMap;

use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, is_benchmark_ticker, BASE_CURRENCY, DATE_FORMAT};
use crate::db::repos::exchange_rate_repo;
use crate::models::Asset;
use crate::services::price::PriceFetcher;

pub fn currency_pair(from: &str) -> String {
    format!("{from}{BASE_CURRENCY}")
}

pub async fn get_exchange_rate(
    db: &DatabaseConnection,
    pair: &str,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    if let Some(rate) = exchange_rate_repo::find_rate(db, pair, date).await? {
        return Ok(Some(rate));
    }
    exchange_rate_repo::find_rate_at_or_before(db, pair, date).await
}

/// Fetches the latest exchange rate from the API without persisting to DB.
pub async fn fetch_live_rate(
    pair: &str,
    date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<f64>> {
    let rates = price_fetcher
        .get_historical_exchange_rates(pair, date, date)
        .await?;
    Ok(rates.last().map(|(_, rate)| *rate))
}

/// Fetches live exchange rates for all non-EUR currencies (excluding benchmark assets)
/// in parallel. Returns a map of currency pair -> rate for pairs where the fetch succeeded.
pub async fn fetch_live_rates_batch(
    assets: &[Asset],
    today: &str,
    price_fetcher: &dyn PriceFetcher,
) -> HashMap<String, f64> {
    let fx_pairs: Vec<String> = assets
        .iter()
        .filter(|a| a.currency != BASE_CURRENCY && !is_benchmark_ticker(&a.ticker))
        .map(|a| currency_pair(&a.currency))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    let futures: Vec<_> = fx_pairs
        .iter()
        .map(|pair| async {
            let result = fetch_live_rate(pair, today, price_fetcher).await;
            (pair.clone(), result)
        })
        .collect();
    futures::future::join_all(futures)
        .await
        .into_iter()
        .filter_map(|(pair, r)| r.ok().flatten().map(|rate| (pair, rate)))
        .collect()
}

pub async fn fill_rates_for_range(
    db: &DatabaseConnection,
    pair: &str,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<String>> {
    let rates = price_fetcher
        .get_historical_exchange_rates(pair, start_date, end_date)
        .await;

    let rate_map: std::collections::HashMap<String, f64> = match rates {
        Ok(rates) => rates.into_iter().collect(),
        Err(e) => {
            tracing::warn!(%pair, error = %e, "failed to fetch exchange rates");
            return Ok(None);
        }
    };

    let last_api_date = match rate_map.keys().max() {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let start = NaiveDate::parse_from_str(start_date, DATE_FORMAT).context("invalid start date")?;
    let fill_end =
        NaiveDate::parse_from_str(&last_api_date, DATE_FORMAT).context("invalid last API date")?;

    let mut last_known_rate = exchange_rate_repo::find_rate_before(db, pair, start_date).await?;

    let mut current = start;
    while current <= fill_end {
        let date_str = format_date(current);

        if let Some(&api_rate) = rate_map.get(&date_str) {
            exchange_rate_repo::upsert(db, pair, &date_str, api_rate).await?;
            last_known_rate = Some(api_rate);
        } else if let Some(fill_rate) = last_known_rate {
            if !exchange_rate_repo::exists(db, pair, &date_str).await? {
                exchange_rate_repo::upsert(db, pair, &date_str, fill_rate).await?;
            }
        }

        current += chrono::Duration::days(1);
    }

    Ok(Some(last_api_date))
}
