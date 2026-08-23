use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use super::{policy, MarketData, NavValuationData, SourceObservation};
use crate::constants::{format_date, BASE_CURRENCY, FUND_API_PADDING_DAYS};
use crate::db::repos::{daily_price_repo, exchange_rate_repo};
use crate::models::{Asset, AssetType, ValuationMarketData, ValuationMarketDataAvailability};

struct LatestMarketDataDate {
    date: String,
}

struct FilledValues {
    latest: LatestMarketDataDate,
    values: BTreeMap<NaiveDate, f64>,
}

struct PreparedHistoricalMarketData {
    availability: ValuationMarketDataAvailability,
    valuation_data: NavValuationData,
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

pub(crate) async fn get_base_currency_price_series_for_assets(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<HashMap<i32, crate::models::BaseCurrencyPriceSeries>> {
    let asset_ids: Vec<i32> = assets.iter().map(|asset| asset.id).collect();
    let mut prices =
        daily_price_repo::find_prices_between_assets(db, &asset_ids, start_date, end_date).await?;
    let currencies: Vec<String> = assets
        .iter()
        .filter(|asset| asset.currency != BASE_CURRENCY)
        .map(|asset| asset.currency.clone())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    let rates = exchange_rate_repo::find_rates_between_currencies(
        db,
        &currencies,
        BASE_CURRENCY,
        start_date,
        end_date,
    )
    .await?;

    let mut series = HashMap::with_capacity(assets.len());
    for asset in assets {
        let asset_prices = prices.remove(&asset.id).unwrap_or_default();
        if asset.currency == BASE_CURRENCY {
            series.insert(asset.id, asset_prices);
            continue;
        }

        let rate_map: HashMap<&str, f64> = rates
            .get(&asset.currency)
            .into_iter()
            .flatten()
            .map(|(date, rate)| (date.as_str(), *rate))
            .collect();
        series.insert(
            asset.id,
            asset_prices
                .into_iter()
                .filter_map(|(date, price)| {
                    rate_map.get(date.as_str()).map(|rate| (date, price * rate))
                })
                .collect(),
        );
    }
    Ok(series)
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

async fn prepare_historical_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<ValuationMarketData> {
    let prepared = prepare_historical_market_data_inner(
        db,
        assets,
        start_date,
        end_date,
        market_data,
        true,
        false,
    )
    .await?;
    Ok(ValuationMarketData {
        effective_end: prepared.availability.effective_end,
        limitations: prepared.availability.limitations,
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
    let prepared = prepare_historical_market_data_inner(
        db,
        assets,
        start_date,
        end_date,
        market_data,
        false,
        false,
    )
    .await?;
    Ok(prepared.availability)
}

pub(crate) async fn prepare_valuation_market_data_for_nav(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<(ValuationMarketDataAvailability, NavValuationData)> {
    let prepared = prepare_historical_market_data_inner(
        db,
        assets,
        start_date,
        end_date,
        market_data,
        false,
        true,
    )
    .await?;
    Ok((prepared.availability, prepared.valuation_data))
}

async fn prepare_historical_market_data_inner(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
    strict: bool,
    preload: bool,
) -> anyhow::Result<PreparedHistoricalMarketData> {
    let requested_end =
        policy::parse_market_data_date(end_date, "historical market data end date")?;
    let cache_end = format_date(requested_end.min(market_data.today() - chrono::Duration::days(1)));
    let currencies = infer_required_currencies(assets);
    let latest_asset_dates =
        fill_nav_asset_prices(db, assets, start_date, &cache_end, market_data).await?;
    let latest_rate_dates = if currencies.is_empty() {
        HashMap::new()
    } else {
        fill_nav_exchange_rates(db, &currencies, start_date, &cache_end, market_data).await?
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
            policy::parse_market_data_date(&latest_date.latest.date, "asset price date")?;
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
            policy::parse_market_data_date(&latest_date.latest.date, "FX rate date")?;
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

    Ok(PreparedHistoricalMarketData {
        availability: ValuationMarketDataAvailability {
            effective_end,
            limitations,
            data_available,
        },
        valuation_data: if preload {
            NavValuationData::from_maps(
                latest_asset_dates
                    .into_iter()
                    .map(|(asset_id, values)| (asset_id, values.values))
                    .collect(),
                latest_rate_dates
                    .into_iter()
                    .map(|(currency, values)| (currency, values.values))
                    .collect(),
            )
        } else {
            NavValuationData::from_maps(HashMap::new(), HashMap::new())
        },
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

async fn fill_nav_asset_prices(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<HashMap<i32, FilledValues>> {
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
    let mut latest_dates: HashMap<i32, FilledValues> = HashMap::new();
    for (asset, result) in results {
        match result {
            Ok(Some(values)) => {
                latest_dates.insert(asset.id, values);
            }
            Ok(None) => {
                tracing::warn!(ticker = %asset.ticker, "no new price data from API, falling back to latest cached date");
                tracing::warn!(ticker = %asset.ticker, "no cached price data available at all");
            }
            Err(e) => {
                tracing::warn!(ticker = %asset.ticker, error = %e, "failed to fill prices, falling back to latest cached date");
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
) -> anyhow::Result<Option<FilledValues>> {
    let start = policy::parse_market_data_date(start_date, "historical asset price start date")?;
    let requested_end =
        policy::parse_market_data_date(end_date, "historical asset price end date")?;
    let latest_completed_date = market_data.today() - chrono::Duration::days(1);
    let cached =
        daily_price_repo::find_coverage_with_seed(db, asset.id, start_date, end_date).await?;
    let mut known: BTreeMap<NaiveDate, f64> = cached
        .iter()
        .map(|(date, value)| {
            Ok((
                policy::parse_market_data_date(date, "cached historical asset price date")?,
                *value,
            ))
        })
        .collect::<anyhow::Result<_>>()?;
    let cached_dates = known.keys().copied().collect::<HashSet<_>>();
    let intervals = missing_intervals(start, requested_end, &cached_dates);
    let mut writes = Vec::new();
    for interval in intervals {
        let source_start = historical_source_start_date(asset, interval.start)?;
        let observations = match fetch_asset_price_history(
            market_data,
            asset,
            lookup_identifier,
            source_start,
            interval.end,
        )
        .await
        {
            Ok(observations) => observations,
            Err(error) => {
                tracing::warn!(ticker = %asset.ticker, error = %format!("{error:#}"), "failed to fetch historical prices");
                continue;
            }
        };
        append_asset_interval_writes(
            asset.id,
            interval,
            observations,
            latest_completed_date,
            &mut known,
            &mut writes,
        );
    }
    if !writes.is_empty() {
        daily_price_repo::insert_many_immutable(db, &writes).await?;
    }
    let latest_date = known
        .range(..=requested_end)
        .next_back()
        .map(|(date, _)| *date);
    Ok(latest_date.map(|date| FilledValues {
        latest: LatestMarketDataDate {
            date: format_date(date),
        },
        values: known,
    }))
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

fn historical_source_start_date(asset: &Asset, start_date: NaiveDate) -> anyhow::Result<NaiveDate> {
    if matches!(asset.asset_type, AssetType::Fund | AssetType::Etf) {
        return start_date
            .checked_sub_signed(chrono::Duration::days(FUND_API_PADDING_DAYS))
            .context("historical asset source start date underflow");
    }
    Ok(start_date)
}

async fn fill_nav_exchange_rates(
    db: &DatabaseConnection,
    currencies: &[String],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<HashMap<String, FilledValues>> {
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
            Ok(Some(values)) => {
                latest_dates.insert(currency.clone(), values);
            }
            Ok(None) => {
                tracing::warn!(%currency, to_currency = BASE_CURRENCY, "no new exchange rate data from API, falling back to latest cached date");
                tracing::warn!(%currency, to_currency = BASE_CURRENCY, "no cached exchange rate data available at all");
            }
            Err(e) => {
                tracing::warn!(%currency, to_currency = BASE_CURRENCY, error = %e, "failed to fill exchange rates, falling back to latest cached date");
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
) -> anyhow::Result<Option<FilledValues>> {
    let start = policy::parse_market_data_date(start_date, "historical FX start date")?;
    let requested_end = policy::parse_market_data_date(end_date, "historical FX end date")?;
    let latest_completed_date = market_data.today() - chrono::Duration::days(1);
    let cached = exchange_rate_repo::find_coverage_with_seed(
        db,
        from_currency,
        BASE_CURRENCY,
        start_date,
        end_date,
    )
    .await?;
    let mut known: BTreeMap<NaiveDate, f64> = cached
        .iter()
        .map(|(date, value)| {
            Ok((
                policy::parse_market_data_date(date, "cached historical FX date")?,
                *value,
            ))
        })
        .collect::<anyhow::Result<_>>()?;
    let cached_dates = known.keys().copied().collect::<HashSet<_>>();
    let intervals = missing_intervals(start, requested_end, &cached_dates);
    let mut writes = Vec::new();
    for interval in intervals {
        let observations = match market_data
            .exchange_rate_history(from_currency, BASE_CURRENCY, interval.start, interval.end)
            .await
        {
            Ok(observations) => observations,
            Err(error) => {
                tracing::warn!(%from_currency, to_currency = BASE_CURRENCY, error = %error, "failed to fetch exchange rates");
                continue;
            }
        };
        append_fx_interval_writes(
            from_currency,
            interval,
            observations,
            latest_completed_date,
            &mut known,
            &mut writes,
        );
    }
    if !writes.is_empty() {
        exchange_rate_repo::insert_many_immutable(db, &writes).await?;
    }
    let latest_date = known
        .range(..=requested_end)
        .next_back()
        .map(|(date, _)| *date);
    Ok(latest_date.map(|date| FilledValues {
        latest: LatestMarketDataDate {
            date: format_date(date),
        },
        values: known,
    }))
}

#[derive(Clone, Copy)]
struct DateInterval {
    start: NaiveDate,
    end: NaiveDate,
}

fn missing_intervals(
    start: NaiveDate,
    end: NaiveDate,
    cached_dates: &HashSet<NaiveDate>,
) -> Vec<DateInterval> {
    let mut intervals = Vec::new();
    let mut missing_start = None;
    let mut current = start;
    while current <= end {
        if cached_dates.contains(&current) {
            if let Some(interval_start) = missing_start.take() {
                intervals.push(DateInterval {
                    start: interval_start,
                    end: current - chrono::Duration::days(1),
                });
            }
        } else if missing_start.is_none() {
            missing_start = Some(current);
        }
        current += chrono::Duration::days(1);
    }
    if let Some(interval_start) = missing_start {
        intervals.push(DateInterval {
            start: interval_start,
            end,
        });
    }
    intervals
}

fn append_asset_interval_writes(
    asset_id: i32,
    interval: DateInterval,
    observations: Vec<SourceObservation>,
    latest_completed_date: NaiveDate,
    known: &mut BTreeMap<NaiveDate, f64>,
    writes: &mut Vec<daily_price_repo::DailyPriceWrite>,
) {
    writes.extend(
        forward_filled_interval(interval, observations, latest_completed_date, known)
            .into_iter()
            .map(|(date, value)| daily_price_repo::DailyPriceWrite {
                asset_id,
                date: format_date(date),
                price: value,
                is_api_failure: false,
            }),
    );
}

fn append_fx_interval_writes(
    from_currency: &str,
    interval: DateInterval,
    observations: Vec<SourceObservation>,
    latest_completed_date: NaiveDate,
    known: &mut BTreeMap<NaiveDate, f64>,
    writes: &mut Vec<exchange_rate_repo::ExchangeRateWrite>,
) {
    writes.extend(
        forward_filled_interval(interval, observations, latest_completed_date, known)
            .into_iter()
            .map(|(date, value)| exchange_rate_repo::ExchangeRateWrite {
                from_currency: from_currency.to_owned(),
                to_currency: BASE_CURRENCY.to_owned(),
                date: format_date(date),
                rate: value,
            }),
    );
}

fn forward_filled_interval(
    interval: DateInterval,
    observations: Vec<SourceObservation>,
    latest_completed_date: NaiveDate,
    known: &mut BTreeMap<NaiveDate, f64>,
) -> Vec<(NaiveDate, f64)> {
    let source_values = interval_source_values(interval, observations, latest_completed_date);
    let has_later_cached_value = known
        .range((interval.end + chrono::Duration::days(1))..)
        .next()
        .is_some();
    let fill_end = if has_later_cached_value {
        interval.end
    } else if let Some(source_end) = source_values.keys().next_back().copied() {
        source_end
    } else {
        return Vec::new();
    };
    let mut last_known = known
        .range(..interval.start)
        .next_back()
        .map(|(_, value)| *value);
    let mut filled = Vec::new();
    let mut current = interval.start;
    while current <= fill_end {
        if let Some(value) = source_values.get(&current).copied().or(last_known) {
            filled.push((current, value));
            known.insert(current, value);
            last_known = Some(value);
        }
        current += chrono::Duration::days(1);
    }
    filled
}

fn interval_source_values(
    interval: DateInterval,
    observations: Vec<SourceObservation>,
    latest_completed_date: NaiveDate,
) -> BTreeMap<NaiveDate, f64> {
    observations
        .into_iter()
        .filter(|observation| {
            observation.date >= interval.start
                && observation.date <= interval.end
                && observation.date <= latest_completed_date
        })
        .map(|observation| (observation.date, observation.value))
        .collect()
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
