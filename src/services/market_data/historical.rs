use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use super::{policy, MarketData, SourceObservation};
use crate::constants::{format_date, BASE_CURRENCY, FUND_API_PADDING_DAYS};
use crate::db::repos::{daily_price_repo, exchange_rate_repo};
use crate::models::{
    Asset, AssetType, MarketDataValuation, ValuationMarketData, ValuationMarketDataAvailability,
};

struct LatestMarketDataDate {
    date: String,
}

pub(crate) async fn prepare_valuation_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<ValuationMarketData> {
    prepare_historical_market_data(db, assets, start_date, end_date, market_data).await
}

pub(crate) async fn fill_historical_market_data_cache(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<()> {
    fill_nav_asset_prices(db, assets, start_date, end_date, market_data).await?;
    let currencies = infer_required_currencies(assets);
    if !currencies.is_empty() {
        fill_nav_exchange_rates(db, &currencies, start_date, end_date, market_data).await?;
    }
    Ok(())
}

pub(crate) async fn get_required_asset_valuation_data(
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

async fn get_asset_valuation_data(
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

pub(crate) async fn get_exchange_rate_for_asset(
    db: &DatabaseConnection,
    asset: &Asset,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    if asset.currency == BASE_CURRENCY {
        return Ok(Some(1.0));
    }

    get_exchange_rate(db, &asset.currency, date).await
}

pub(crate) async fn get_required_asset_exchange_rates(
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

pub(crate) async fn get_base_currency_price_series(
    db: &DatabaseConnection,
    asset: &Asset,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let prices = daily_price_repo::find_prices_between(db, asset.id, start_date, end_date).await?;
    if asset.currency == BASE_CURRENCY {
        return Ok(prices);
    }

    let rates = exchange_rate_repo::find_rates_between(
        db,
        &asset.currency,
        BASE_CURRENCY,
        start_date,
        end_date,
    )
    .await?;
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

async fn get_exchange_rate(
    db: &DatabaseConnection,
    from_currency: &str,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    if let Some(rate) =
        exchange_rate_repo::find_rate(db, from_currency, BASE_CURRENCY, date).await?
    {
        return Ok(Some(rate));
    }

    exchange_rate_repo::find_rate_at_or_before(db, from_currency, BASE_CURRENCY, date).await
}

async fn get_closing_price(
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
    market_data: &MarketData,
) -> anyhow::Result<ValuationMarketData> {
    let availability =
        prepare_historical_market_data_inner(db, assets, start_date, end_date, market_data, true)
            .await?;
    Ok(ValuationMarketData {
        effective_end: availability.effective_end,
        limitations: availability.limitations,
    })
}

/// Same preparation as `prepare_historical_market_data` but unavailable required
/// data is reported as `data_available = false` plus limitations instead of a
/// hard error, so NAV readiness can represent an unavailable valuation scope
/// without masking genuine DB, parsing, or invariant errors.
pub(crate) async fn prepare_valuation_market_data_if_available(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<ValuationMarketDataAvailability> {
    prepare_historical_market_data_inner(db, assets, start_date, end_date, market_data, false).await
}

async fn prepare_historical_market_data_inner(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
    strict: bool,
) -> anyhow::Result<ValuationMarketDataAvailability> {
    let requested_end =
        policy::parse_market_data_date(end_date, "historical market data end date")?;
    let currencies = infer_required_currencies(assets);
    let latest_asset_dates =
        fill_nav_asset_prices(db, assets, start_date, end_date, market_data).await?;
    let latest_rate_dates = if currencies.is_empty() {
        HashMap::new()
    } else {
        fill_nav_exchange_rates(db, &currencies, start_date, end_date, market_data).await?
    };

    let mut latest_required_dates = Vec::with_capacity(assets.len() + currencies.len() + 1);
    latest_required_dates.push(requested_end);
    let mut limitations = Vec::new();
    let mut data_available = true;

    for asset in assets {
        let Some(latest_date) = latest_asset_dates.get(&asset.id) else {
            if strict {
                bail!(
                    "missing required historical market data for asset {} ({})",
                    asset.ticker,
                    asset.name
                );
            }
            data_available = false;
            limitations.push(policy::missing_asset_limitation(asset, requested_end));
            continue;
        };
        let latest_available_date =
            policy::parse_market_data_date(&latest_date.date, "asset price date")?;
        latest_required_dates.push(latest_available_date);
        if let Some(limitation) =
            policy::classify_asset_limitation(asset, latest_available_date, requested_end)
        {
            limitations.push(limitation);
        }
    }

    for currency in currencies {
        let Some(latest_date) = latest_rate_dates.get(&currency) else {
            if strict {
                bail!(
                    "missing required historical market data for FX rate {currency}{BASE_CURRENCY}"
                );
            }
            data_available = false;
            limitations.push(policy::missing_fx_limitation(&currency, requested_end));
            continue;
        };
        let latest_available_date =
            policy::parse_market_data_date(&latest_date.date, "FX rate date")?;
        latest_required_dates.push(latest_available_date);
        if let Some(limitation) =
            policy::classify_fx_limitation(&currency, latest_available_date, requested_end)
        {
            limitations.push(limitation);
        }
    }

    let effective_end = latest_required_dates
        .into_iter()
        .min()
        .context("historical market data preparation had no date requirements")?;

    Ok(ValuationMarketDataAvailability {
        effective_end,
        limitations,
        data_available,
    })
}

fn infer_required_currencies(assets: &[Asset]) -> Vec<String> {
    let mut currencies: Vec<String> = assets
        .iter()
        .filter(|asset| asset.currency != BASE_CURRENCY)
        .map(|asset| asset.currency.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    currencies.sort();
    currencies
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
    market_data: &MarketData,
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
                market_data,
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
    market_data: &MarketData,
) -> anyhow::Result<Option<String>> {
    let source_start_date = historical_source_start_date(asset, start_date)?;
    let source_start =
        policy::parse_market_data_date(&source_start_date, "historical asset source start date")?;
    let source_end = policy::parse_market_data_date(end_date, "historical asset source end date")?;
    let prices = fetch_asset_price_history(
        market_data,
        asset,
        lookup_identifier,
        source_start,
        source_end,
    )
    .await;

    let requested_end =
        policy::parse_market_data_date(end_date, "historical asset price end date")?;
    let latest_completed_date = market_data.today() - chrono::Duration::days(1);

    let price_map: HashMap<String, f64> = match prices {
        Ok(prices) => prices
            .into_iter()
            .filter(|observation| {
                observation.date <= requested_end && observation.date <= latest_completed_date
            })
            .map(|observation| (format_date(observation.date), observation.value))
            .collect(),
        Err(e) => {
            tracing::warn!(ticker = %asset.ticker, error = %format!("{e:#}"), "failed to fetch historical prices");
            return Ok(None);
        }
    };

    let last_api_date = match price_map.keys().max() {
        Some(date) => date.clone(),
        None => return Ok(None),
    };

    let start = policy::parse_market_data_date(start_date, "historical asset price start date")?;
    let fill_end =
        policy::parse_market_data_date(&last_api_date, "last historical asset price API date")?;
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

fn historical_source_start_date(asset: &Asset, start_date: &str) -> anyhow::Result<String> {
    if matches!(asset.asset_type, AssetType::Fund | AssetType::Etf) {
        let start =
            policy::parse_market_data_date(start_date, "historical asset source start date")?;
        return Ok(format_date(
            start - chrono::Duration::days(FUND_API_PADDING_DAYS),
        ));
    }
    Ok(start_date.to_owned())
}

async fn fill_nav_exchange_rates(
    db: &DatabaseConnection,
    currencies: &[String],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<HashMap<String, LatestMarketDataDate>> {
    tracing::debug!(currency_count = currencies.len(), %start_date, %end_date, "filling NAV exchange rate cache");

    let futures: Vec<_> = currencies
        .iter()
        .map(|currency| async move {
            let result =
                fill_historical_exchange_rates(db, currency, start_date, end_date, market_data)
                    .await;
            (currency, result)
        })
        .collect();

    let results = futures::future::join_all(futures).await;

    let mut latest_dates = HashMap::new();
    for (currency, result) in results {
        match result {
            Ok(Some(date)) => {
                latest_dates.insert(currency.clone(), LatestMarketDataDate { date });
            }
            Ok(None) => {
                tracing::warn!(%currency, to_currency = BASE_CURRENCY, "no new exchange rate data from API, falling back to latest cached date");
                if let Some(cached) =
                    exchange_rate_repo::find_latest_date(db, currency, BASE_CURRENCY).await?
                {
                    latest_dates.insert(currency.clone(), LatestMarketDataDate { date: cached });
                } else {
                    tracing::warn!(%currency, to_currency = BASE_CURRENCY, "no cached exchange rate data available at all");
                }
            }
            Err(e) => {
                tracing::warn!(%currency, to_currency = BASE_CURRENCY, error = %e, "failed to fill exchange rates, falling back to latest cached date");
                if let Some(cached) =
                    exchange_rate_repo::find_latest_date(db, currency, BASE_CURRENCY).await?
                {
                    latest_dates.insert(currency.clone(), LatestMarketDataDate { date: cached });
                }
            }
        }
    }

    Ok(latest_dates)
}

async fn fill_historical_exchange_rates(
    db: &DatabaseConnection,
    from_currency: &str,
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<Option<String>> {
    let source_start =
        policy::parse_market_data_date(start_date, "historical FX source start date")?;
    let source_end = policy::parse_market_data_date(end_date, "historical FX source end date")?;
    let rates = market_data
        .exchange_rate_history(from_currency, BASE_CURRENCY, source_start, source_end)
        .await;

    let requested_end = policy::parse_market_data_date(end_date, "historical FX end date")?;
    let latest_completed_date = market_data.today() - chrono::Duration::days(1);

    let rate_map: HashMap<String, f64> = match rates {
        Ok(rates) => rates
            .into_iter()
            .filter(|observation| {
                observation.date <= requested_end && observation.date <= latest_completed_date
            })
            .map(|observation| (format_date(observation.date), observation.value))
            .collect(),
        Err(e) => {
            tracing::warn!(%from_currency, to_currency = BASE_CURRENCY, error = %e, "failed to fetch exchange rates");
            return Ok(None);
        }
    };

    let last_api_date = match rate_map.keys().max() {
        Some(date) => date.clone(),
        None => return Ok(None),
    };

    let start = policy::parse_market_data_date(start_date, "historical FX start date")?;
    let fill_end = policy::parse_market_data_date(&last_api_date, "last historical FX API date")?;
    let mut last_known_rate =
        exchange_rate_repo::find_rate_before(db, from_currency, BASE_CURRENCY, start_date).await?;

    let mut current = start;
    while current <= fill_end {
        let date_str = format_date(current);

        if let Some(&api_rate) = rate_map.get(&date_str) {
            exchange_rate_repo::upsert(db, from_currency, BASE_CURRENCY, &date_str, api_rate)
                .await?;
            last_known_rate = Some(api_rate);
        } else if let Some(fill_rate) = last_known_rate {
            if !exchange_rate_repo::exists(db, from_currency, BASE_CURRENCY, &date_str).await? {
                exchange_rate_repo::upsert(db, from_currency, BASE_CURRENCY, &date_str, fill_rate)
                    .await?;
            }
        }

        current += chrono::Duration::days(1);
    }

    Ok(Some(last_api_date))
}

async fn fetch_asset_price_history(
    market_data: &MarketData,
    asset: &Asset,
    lookup_identifier: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> anyhow::Result<Vec<SourceObservation>> {
    match asset.asset_type {
        AssetType::Stock => {
            market_data
                .stock_price_history(lookup_identifier, start, end)
                .await
        }
        AssetType::Fund | AssetType::Etf => {
            market_data
                .fund_price_history(lookup_identifier, start, end)
                .await
        }
    }
}
