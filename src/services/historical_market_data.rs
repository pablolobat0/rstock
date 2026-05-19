use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, BASE_CURRENCY, DATE_FORMAT};
use crate::db::repos::{daily_price_repo, exchange_rate_repo};
use crate::models::{Asset, AssetType, BenchmarkMarketData, MarketDataValuation, NavMarketData};
use crate::services::market_data_policy;
use crate::services::price::PriceFetcher;

struct LatestMarketDataDate {
    date: String,
}

pub async fn prepare_nav_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<NavMarketData> {
    prepare_historical_market_data(db, assets, start_date, end_date, price_fetcher).await
}

pub async fn prepare_benchmark_market_data(
    db: &DatabaseConnection,
    benchmark: &Asset,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<BenchmarkMarketData> {
    let market_data = prepare_historical_market_data(
        db,
        std::slice::from_ref(benchmark),
        start_date,
        end_date,
        price_fetcher,
    )
    .await?;

    Ok(BenchmarkMarketData {
        asset_id: benchmark.id,
        effective_end: market_data.effective_end,
        limitations: market_data.limitations,
    })
}

pub async fn get_required_asset_valuation_data(
    db: &DatabaseConnection,
    asset: &Asset,
    date: &str,
) -> anyhow::Result<MarketDataValuation> {
    get_asset_valuation_data(db, asset, date)
        .await?
        .with_context(|| {
            format!(
                "missing required historical market data for asset {} ({})",
                asset.ticker, asset.name
            )
        })
}

pub async fn get_asset_valuation_data(
    db: &DatabaseConnection,
    asset: &Asset,
    date: &str,
) -> anyhow::Result<Option<MarketDataValuation>> {
    let Some(native_price) = get_closing_price(db, asset, date).await? else {
        return Ok(None);
    };
    let fx_rate = get_required_exchange_rate_for_asset(db, asset, date).await?;

    Ok(Some(MarketDataValuation {
        native_price,
        fx_rate,
        base_currency_price: native_price * fx_rate,
    }))
}

pub async fn get_exchange_rate_for_asset(
    db: &DatabaseConnection,
    asset: &Asset,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    if asset.currency == BASE_CURRENCY {
        return Ok(Some(1.0));
    }

    let pair = market_data_policy::currency_pair(&asset.currency);
    get_exchange_rate(db, &pair, date).await
}

pub async fn get_required_asset_exchange_rates(
    db: &DatabaseConnection,
    assets: &[Asset],
    date: &str,
) -> anyhow::Result<HashMap<i32, f64>> {
    let mut rates = HashMap::new();
    for asset in assets {
        rates.insert(
            asset.id,
            get_required_exchange_rate_for_asset(db, asset, date).await?,
        );
    }
    Ok(rates)
}

pub async fn get_base_currency_price_series(
    db: &DatabaseConnection,
    asset: &Asset,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let prices = daily_price_repo::find_prices_between(db, asset.id, start_date, end_date).await?;
    if asset.currency == BASE_CURRENCY {
        return Ok(prices);
    }

    let pair = market_data_policy::currency_pair(&asset.currency);
    let rates = exchange_rate_repo::find_rates_between(db, &pair, start_date, end_date).await?;
    let rate_map: HashMap<&str, f64> = rates
        .iter()
        .map(|(date, rate)| (date.as_str(), *rate))
        .collect();

    Ok(prices
        .iter()
        .filter_map(|(date, price)| {
            rate_map
                .get(date.as_str())
                .map(|rate| (date.clone(), price * rate))
        })
        .collect())
}

pub async fn fetch_direct_base_currency_price_series(
    asset: &Asset,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Vec<(String, f64)>> {
    let lookup_identifier = lookup_identifier(asset)?;
    let prices = filter_fetched_series(
        price_fetcher
            .get_historical_prices(lookup_identifier, start_date, end_date, &asset.asset_type)
            .await?,
        start_date,
        end_date,
    );

    if prices.is_empty() {
        bail!("no price history returned for '{}'", asset.ticker);
    }

    if asset.currency == BASE_CURRENCY {
        return Ok(prices);
    }

    let pair = market_data_policy::currency_pair(&asset.currency);
    let rates = filter_fetched_series(
        price_fetcher
            .get_historical_exchange_rates(&pair, start_date, end_date)
            .await?,
        start_date,
        end_date,
    );

    if rates.is_empty() {
        bail!("no FX history returned for currency '{}'", asset.currency);
    }

    let rate_map: HashMap<&str, f64> = rates
        .iter()
        .map(|(date, rate)| (date.as_str(), *rate))
        .collect();
    let eur_prices: Vec<(String, f64)> = prices
        .iter()
        .filter_map(|(date, price)| {
            rate_map
                .get(date.as_str())
                .map(|rate| (date.clone(), price * rate))
        })
        .collect();

    if eur_prices.is_empty() {
        bail!(
            "could not align price and FX history for '{}'",
            asset.ticker
        );
    }

    Ok(eur_prices)
}

async fn get_exchange_rate(
    db: &DatabaseConnection,
    pair: &str,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    if let Some(rate) = exchange_rate_repo::find_rate(db, pair, date).await? {
        return Ok(Some(rate));
    }

    exchange_rate_repo::find_rate_at_or_before(db, pair, date).await
}

pub async fn get_closing_price(
    db: &DatabaseConnection,
    asset: &Asset,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    if let Some(price) = daily_price_repo::find_price(db, asset.id, date).await? {
        return Ok(Some(price));
    }

    daily_price_repo::find_price_at_or_before(db, asset.id, date).await
}

async fn prepare_historical_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<NavMarketData> {
    let requested_end =
        market_data_policy::parse_market_data_date(end_date, "historical market data end date")?;
    let currency_pairs = infer_required_currency_pairs(assets);
    let latest_asset_dates =
        fill_nav_asset_prices(db, assets, start_date, end_date, price_fetcher).await?;
    let latest_rate_dates = if currency_pairs.is_empty() {
        HashMap::new()
    } else {
        fill_nav_exchange_rates(db, &currency_pairs, start_date, end_date, price_fetcher).await?
    };

    let mut latest_required_dates = Vec::with_capacity(assets.len() + currency_pairs.len() + 1);
    latest_required_dates.push(requested_end);
    let mut limitations = Vec::new();

    for asset in assets {
        let Some(latest_date) = latest_asset_dates.get(&asset.id) else {
            bail!(
                "missing required historical market data for asset {} ({})",
                asset.ticker,
                asset.name
            );
        };
        let latest_available_date =
            market_data_policy::parse_market_data_date(&latest_date.date, "asset price date")?;
        latest_required_dates.push(latest_available_date);
        if let Some(limitation) = market_data_policy::classify_asset_limitation(
            asset,
            latest_available_date,
            requested_end,
        ) {
            limitations.push(limitation);
        }
    }

    for pair in currency_pairs {
        let Some(latest_date) = latest_rate_dates.get(&pair) else {
            bail!("missing required historical market data for FX rate {pair}");
        };
        let latest_available_date =
            market_data_policy::parse_market_data_date(&latest_date.date, "FX rate date")?;
        latest_required_dates.push(latest_available_date);
        let currency = pair.strip_suffix(BASE_CURRENCY).unwrap_or(&pair);
        if let Some(limitation) = market_data_policy::classify_fx_limitation(
            currency,
            latest_available_date,
            requested_end,
        ) {
            limitations.push(limitation);
        }
    }

    let effective_end = latest_required_dates
        .into_iter()
        .min()
        .context("historical market data preparation had no date requirements")?;

    Ok(NavMarketData {
        effective_end,
        limitations,
    })
}

fn infer_required_currency_pairs(assets: &[Asset]) -> Vec<String> {
    let mut pairs: Vec<String> = assets
        .iter()
        .filter(|asset| asset.currency != BASE_CURRENCY)
        .map(|asset| market_data_policy::currency_pair(&asset.currency))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    pairs.sort();
    pairs
}

async fn get_required_exchange_rate_for_asset(
    db: &DatabaseConnection,
    asset: &Asset,
    date: &str,
) -> anyhow::Result<f64> {
    get_exchange_rate_for_asset(db, asset, date)
        .await?
        .with_context(|| {
            format!(
                "missing required historical market data for FX rate for asset {} ({})",
                asset.ticker, asset.name
            )
        })
}

async fn fill_nav_asset_prices(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<HashMap<i32, LatestMarketDataDate>> {
    tracing::debug!(asset_count = assets.len(), %start_date, %end_date, "filling NAV asset price cache");

    let mut requirements = Vec::with_capacity(assets.len());
    for asset in assets {
        requirements.push((asset, lookup_identifier(asset)?));
    }

    let futures: Vec<_> = requirements
        .iter()
        .map(|(asset, lookup_identifier)| async move {
            let result = fill_historical_asset_prices(
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

    let mut latest_dates: HashMap<i32, LatestMarketDataDate> = HashMap::new();
    for (asset, result) in results {
        match result {
            Ok(Some(date)) => {
                latest_dates.insert(asset.id, LatestMarketDataDate { date });
            }
            Ok(None) => {
                tracing::warn!(ticker = %asset.ticker, "no new price data from API, falling back to latest cached date");
                if let Some(cached) = daily_price_repo::find_latest_date(db, asset.id).await? {
                    latest_dates.insert(asset.id, LatestMarketDataDate { date: cached });
                } else {
                    tracing::warn!(ticker = %asset.ticker, "no cached price data available at all");
                }
            }
            Err(e) => {
                tracing::warn!(ticker = %asset.ticker, error = %e, "failed to fill prices, falling back to latest cached date");
                if let Some(cached) = daily_price_repo::find_latest_date(db, asset.id).await? {
                    latest_dates.insert(asset.id, LatestMarketDataDate { date: cached });
                }
            }
        }
    }

    Ok(latest_dates)
}

async fn fill_historical_asset_prices(
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
        market_data_policy::parse_market_data_date(end_date, "historical asset price end date")?;
    let latest_completed_date = chrono::Local::now().date_naive() - chrono::Duration::days(1);

    let price_map: HashMap<String, f64> = match prices {
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
        Some(date) => date.clone(),
        None => return Ok(None),
    };

    let start = market_data_policy::parse_market_data_date(
        start_date,
        "historical asset price start date",
    )?;
    let fill_end = market_data_policy::parse_market_data_date(
        &last_api_date,
        "last historical asset price API date",
    )?;
    let mut last_known_price =
        daily_price_repo::find_price_before(db, asset.id, start_date).await?;

    let mut current = start;
    while current <= fill_end {
        let date_str = format_date(current);

        if let Some(&api_price) = price_map.get(&date_str) {
            daily_price_repo::upsert(db, asset.id, &date_str, api_price, false).await?;
            last_known_price = Some(api_price);
        } else if let Some(fill_price) = last_known_price {
            if !daily_price_repo::exists(db, asset.id, &date_str).await? {
                daily_price_repo::upsert(db, asset.id, &date_str, fill_price, false).await?;
            }
        }

        current += chrono::Duration::days(1);
    }

    Ok(Some(last_api_date))
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

fn filter_fetched_series(
    mut series: Vec<(String, f64)>,
    start_date: &str,
    end_date: &str,
) -> Vec<(String, f64)> {
    series.retain(|(date, _)| date.as_str() >= start_date && date.as_str() <= end_date);
    series.sort_by(|left, right| left.0.cmp(&right.0));
    series.dedup_by(|left, right| left.0 == right.0);
    series
}

async fn fill_nav_exchange_rates(
    db: &DatabaseConnection,
    pairs: &[String],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<HashMap<String, LatestMarketDataDate>> {
    tracing::debug!(pair_count = pairs.len(), %start_date, %end_date, "filling NAV exchange rate cache");

    let futures: Vec<_> = pairs
        .iter()
        .map(|pair| async move {
            let result =
                fill_historical_exchange_rates(db, pair, start_date, end_date, price_fetcher).await;
            (pair, result)
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let mut latest_dates = HashMap::new();
    for (pair, result) in results {
        match result {
            Ok(Some(date)) => {
                latest_dates.insert(pair.clone(), LatestMarketDataDate { date });
            }
            Ok(None) => {
                tracing::warn!(%pair, "no new exchange rate data from API, falling back to latest cached date");
                if let Some(cached) = exchange_rate_repo::find_latest_date(db, pair).await? {
                    latest_dates.insert(pair.clone(), LatestMarketDataDate { date: cached });
                } else {
                    tracing::warn!(%pair, "no cached exchange rate data available at all");
                }
            }
            Err(e) => {
                tracing::warn!(%pair, error = %e, "failed to fill exchange rates, falling back to latest cached date");
                if let Some(cached) = exchange_rate_repo::find_latest_date(db, pair).await? {
                    latest_dates.insert(pair.clone(), LatestMarketDataDate { date: cached });
                }
            }
        }
    }

    Ok(latest_dates)
}

async fn fill_historical_exchange_rates(
    db: &DatabaseConnection,
    pair: &str,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<String>> {
    let rates = price_fetcher
        .get_historical_exchange_rates(pair, start_date, end_date)
        .await;

    let requested_end =
        market_data_policy::parse_market_data_date(end_date, "historical FX end date")?;
    let latest_completed_date = chrono::Local::now().date_naive() - chrono::Duration::days(1);

    let rate_map: HashMap<String, f64> = match rates {
        Ok(rates) => rates
            .into_iter()
            .filter(|(date, _)| {
                NaiveDate::parse_from_str(date, DATE_FORMAT)
                    .is_ok_and(|date| date <= requested_end && date <= latest_completed_date)
            })
            .collect(),
        Err(e) => {
            tracing::warn!(%pair, error = %e, "failed to fetch exchange rates");
            return Ok(None);
        }
    };

    let last_api_date = match rate_map.keys().max() {
        Some(date) => date.clone(),
        None => return Ok(None),
    };

    let start = market_data_policy::parse_market_data_date(start_date, "historical FX start date")?;
    let fill_end =
        market_data_policy::parse_market_data_date(&last_api_date, "last historical FX API date")?;
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
