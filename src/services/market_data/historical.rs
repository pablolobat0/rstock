use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use super::{
    policy, InventoryMarketDataRequirement, MarketData, NavValuationData, NavValuationInterval,
    SourceObservation,
};
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

pub(crate) async fn fill_individual_price_market_data_cache(
    db: &DatabaseConnection,
    requirements: &[InventoryMarketDataRequirement],
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<()> {
    let end = policy::parse_market_data_date(end_date, "inventory historical end date")?;
    let price_requirements: Vec<_> = {
        let attempts = market_data
            .nav_price_attempt_ends
            .lock()
            .map_err(|_| anyhow::anyhow!("NAV price preparation mutex poisoned"))?;
        requirements
            .iter()
            .filter(|requirement| {
                attempts
                    .get(&requirement.asset.id)
                    .is_none_or(|attempted_end| *attempted_end < end)
            })
            .collect()
    };
    let prices =
        futures::future::join_all(price_requirements.iter().map(|requirement| async move {
            fill_nav_asset_prices(
                db,
                std::slice::from_ref(&requirement.asset),
                &format_date(requirement.start_date),
                end_date,
                market_data,
            )
            .await
        }))
        .await;
    for result in prices {
        result?;
    }

    // NAV's price scope says nothing about older ledger FX, which inventory
    // still needs for acquisition costs and dividends. Merge all requirements
    // for a currency before resolving it to avoid overlapping source requests.
    let mut currency_starts = BTreeMap::<String, NaiveDate>::new();
    for requirement in requirements {
        if requirement.asset.currency != BASE_CURRENCY {
            currency_starts
                .entry(requirement.asset.currency.clone())
                .and_modify(|start| *start = (*start).min(requirement.start_date))
                .or_insert(requirement.start_date);
        }
    }
    let rates =
        futures::future::join_all(currency_starts.iter().map(|(currency, start)| async move {
            fill_nav_exchange_rates(
                db,
                std::slice::from_ref(currency),
                &format_date(*start),
                end_date,
                market_data,
            )
            .await
        }))
        .await;
    for result in rates {
        result?;
    }
    Ok(())
}

pub(crate) async fn get_exchange_rates_for_asset(
    db: &DatabaseConnection,
    asset: &Asset,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<BTreeMap<NaiveDate, f64>> {
    if asset.currency == BASE_CURRENCY {
        return Ok(BTreeMap::new());
    }
    let rows = exchange_rate_repo::find_rates_between_currencies(
        db,
        std::slice::from_ref(&asset.currency),
        BASE_CURRENCY,
        start_date,
        end_date,
    )
    .await?;
    let mut rates: BTreeMap<NaiveDate, f64> = rows
        .get(&asset.currency)
        .into_iter()
        .flatten()
        .map(|(date, rate)| {
            Ok((
                policy::parse_market_data_date(date, "historical FX date")?,
                *rate,
            ))
        })
        .collect::<anyhow::Result<_>>()?;
    if let Some((rate, date)) = exchange_rate_repo::find_rate_and_date_at_or_before(
        db,
        &asset.currency,
        BASE_CURRENCY,
        start_date,
    )
    .await?
    {
        rates.insert(
            policy::parse_market_data_date(&date, "historical FX date")?,
            rate,
        );
    }
    Ok(rates)
}

pub(crate) async fn prepare_base_currency_prices(
    db: &DatabaseConnection,
    currency: &str,
    native_prices: &[(String, f64)],
    requested_end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<(Vec<(String, f64)>, Vec<crate::models::MarketDataLimitation>)> {
    for (date, price) in native_prices {
        anyhow::ensure!(
            price.is_finite() && *price > 0.0,
            "invalid successful {currency} fund price on {date}: {price}"
        );
    }
    if currency == BASE_CURRENCY {
        return Ok((native_prices.to_vec(), Vec::new()));
    }
    let requested_end =
        policy::parse_market_data_date(requested_end_date, "price series end date")?;
    let start_date = native_prices
        .first()
        .map_or(requested_end_date, |(date, _)| date.as_str());
    let rates =
        prepare_reference_exchange_rates(db, currency, start_date, requested_end_date, market_data)
            .await?;
    for (date, rate) in &rates {
        anyhow::ensure!(
            rate.is_finite() && *rate > 0.0,
            "invalid persisted {currency}/{BASE_CURRENCY} FX rate on {date}: {rate}"
        );
    }
    let limitations = match rates.range(..=requested_end).next_back() {
        Some((date, _)) => policy::classify_fx_limitation(currency, *date, requested_end)
            .into_iter()
            .collect(),
        None => vec![policy::missing_fx_limitation(currency, requested_end)],
    };
    let mut converted = Vec::with_capacity(native_prices.len());
    for (date, price) in native_prices {
        let parsed = policy::parse_market_data_date(date, "fund price date")?;
        if let Some(rate) = rates.get(&parsed) {
            let base_price = price * rate;
            anyhow::ensure!(
                base_price.is_finite() && base_price > 0.0,
                "invalid converted {currency} fund price on {date}: {base_price}"
            );
            converted.push((date.clone(), base_price));
        }
    }
    Ok((converted, limitations))
}

pub(crate) async fn prepare_reference_exchange_rates(
    db: &DatabaseConnection,
    currency: &str,
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<BTreeMap<NaiveDate, f64>> {
    Ok(
        fill_historical_exchange_rates(db, currency, start_date, end_date, false, market_data)
            .await?
            .map(|values| values.values)
            .unwrap_or_default(),
    )
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

async fn prepare_historical_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<ValuationMarketData> {
    let prepared =
        prepare_historical_market_data_inner(db, assets, start_date, end_date, market_data, true)
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
    let prepared =
        prepare_historical_market_data_inner(db, assets, start_date, end_date, market_data, false)
            .await?;
    Ok(prepared.availability)
}

/// Prepares only the intervals and transaction-date FX range requested by NAV.
/// The returned maps are complete in-memory inputs; execution must not query the
/// cache again when an observation is absent.
pub(crate) async fn prepare_nav_valuation_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    intervals: &[NavValuationInterval],
    fx_currencies: &[String],
    fx_start: Option<NaiveDate>,
    end_date: NaiveDate,
    market_data: &MarketData,
) -> anyhow::Result<NavValuationData> {
    let (exchange_rates, asset_prices) = tokio::join!(
        prepare_nav_exchange_rates(db, fx_currencies, fx_start, end_date, market_data),
        prepare_nav_asset_prices(db, assets, intervals, end_date, market_data),
    );
    Ok(NavValuationData::from_maps(asset_prices?, exchange_rates?))
}

/// Independent assets can prepare together, but one asset's intervals retain
/// their order because they share persisted coverage and predecessor seeds.
/// `MarketData`'s source semaphore bounds the combined asset and FX requests.
async fn prepare_nav_asset_prices(
    db: &DatabaseConnection,
    assets: &[Asset],
    intervals: &[NavValuationInterval],
    end_date: NaiveDate,
    market_data: &MarketData,
) -> anyhow::Result<HashMap<i32, BTreeMap<NaiveDate, f64>>> {
    let asset_map: HashMap<i32, &Asset> = assets.iter().map(|asset| (asset.id, asset)).collect();
    let mut intervals_by_asset = BTreeMap::<i32, Vec<&NavValuationInterval>>::new();
    for interval in intervals {
        intervals_by_asset
            .entry(interval.asset_id)
            .or_default()
            .push(interval);
    }
    let mut requirements = Vec::with_capacity(intervals_by_asset.len());
    for (asset_id, asset_intervals) in intervals_by_asset {
        let asset = asset_map
            .get(&asset_id)
            .with_context(|| format!("missing asset {asset_id} for NAV valuation interval"))?;
        requirements.push((*asset, lookup_identifier(asset)?, asset_intervals));
    }
    let results = futures::future::join_all(requirements.iter().map(
        |(asset, identifier, asset_intervals)| async move {
            let mut prices = BTreeMap::new();
            for interval in asset_intervals {
                if let Some(values) = fill_historical_asset_prices(
                    db,
                    asset,
                    identifier,
                    &format_date(interval.start),
                    AssetFillScope {
                        required_end: interval.end,
                        cache_end: end_date,
                        source_end: end_date,
                        discover_boundary: true,
                    },
                    market_data,
                )
                .await?
                {
                    prices.extend(
                        values
                            .values
                            .into_iter()
                            .filter(|(date, _)| *date <= interval.end),
                    );
                }
            }
            Ok((asset.id, prices))
        },
    ))
    .await;
    results.into_iter().collect()
}

async fn prepare_nav_exchange_rates(
    db: &DatabaseConnection,
    currencies: &[String],
    start_date: Option<NaiveDate>,
    end_date: NaiveDate,
    market_data: &MarketData,
) -> anyhow::Result<HashMap<String, BTreeMap<NaiveDate, f64>>> {
    let Some(start) = start_date else {
        return Ok(HashMap::new());
    };
    let results = futures::future::join_all(currencies.iter().map(|currency| async move {
        let values = fill_historical_exchange_rates(
            db,
            currency,
            &format_date(start),
            &format_date(end_date),
            true,
            market_data,
        )
        .await?
        .map(|values| values.values)
        .unwrap_or_default();
        Ok((currency.clone(), values))
    }))
    .await;
    results.into_iter().collect()
}

async fn prepare_historical_market_data_inner(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
    strict: bool,
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
    let persist_end = policy::parse_market_data_date(end_date, "historical asset price end date")?;

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
                AssetFillScope {
                    required_end: persist_end,
                    cache_end: persist_end,
                    source_end: persist_end,
                    discover_boundary: false,
                },
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
    scope: AssetFillScope,
    market_data: &MarketData,
) -> anyhow::Result<Option<FilledValues>> {
    let start = policy::parse_market_data_date(start_date, "historical asset price start date")?;
    let required_end = scope.required_end;
    let latest_completed_date = market_data.today() - chrono::Duration::days(1);
    let cached = daily_price_repo::find_coverage_with_seed(
        db,
        asset.id,
        start_date,
        &format_date(scope.cache_end),
    )
    .await?;
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
    let intervals = missing_intervals(start, required_end, &cached_dates);
    if intervals.is_empty() {
        let latest_date = known
            .range(..=required_end)
            .next_back()
            .map(|(date, _)| *date);
        return Ok(latest_date.map(|date| FilledValues {
            latest: LatestMarketDataDate {
                date: format_date(date),
            },
            values: known,
        }));
    }
    let mut writes = Vec::new();
    for interval in intervals {
        let source_start = historical_source_start_date(asset, interval.start)?;
        let (observations, predecessor_fetch_failed) = match fetch_asset_price_history_for_interval(
            market_data,
            asset,
            lookup_identifier,
            source_start,
            interval,
            scope,
        )
        .await
        {
            Ok(observations) => observations,
            Err(error) => {
                let usable_cached_price =
                    known
                        .range(..=required_end)
                        .next_back()
                        .is_some_and(|(date, _)| {
                            policy::classify_asset_limitation(asset, *date, required_end).is_none()
                        });
                if usable_cached_price {
                    tracing::debug!(ticker = %asset.ticker, error = %format!("{error:#}"), "failed to fetch historical prices; cached price is within acceptable lag");
                } else {
                    tracing::warn!(ticker = %asset.ticker, error = %format!("{error:#}"), "failed to fetch historical prices");
                }
                continue;
            }
        };
        append_asset_interval_writes(
            asset.id,
            interval,
            observations,
            latest_completed_date,
            required_end,
            if scope.discover_boundary {
                scope.source_end
            } else {
                interval.end
            },
            !predecessor_fetch_failed,
            &mut known,
            &mut writes,
        );
    }
    if !writes.is_empty() {
        daily_price_repo::insert_many_immutable(db, &writes).await?;
    }
    let latest_date = known
        .range(..=required_end)
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
            let result = fill_historical_exchange_rates(
                db,
                currency,
                start_date,
                end_date,
                false,
                market_data,
            )
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
                return Err(e).with_context(|| {
                    format!("failed to prepare {currency}/{BASE_CURRENCY} exchange rates")
                });
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
    discover_predecessor: bool,
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
        let mut observations = observations;
        let mut predecessor_fetch_failed = false;
        if discover_predecessor
            && !observations
                .iter()
                .any(|observation| observation.date < interval.start)
        {
            match market_data
                .latest_exchange_rate_before(from_currency, BASE_CURRENCY, interval.start)
                .await
            {
                Ok(Some(observation)) => observations.push(observation),
                Ok(None) => {}
                Err(error) => {
                    predecessor_fetch_failed = true;
                    tracing::warn!(
                        %from_currency,
                        to_currency = BASE_CURRENCY,
                        error = %format!("{error:#}"),
                        "failed to fetch historical FX predecessor"
                    );
                }
            }
        }
        if !predecessor_fetch_failed {
            preserve_source_predecessor(&observations, interval, latest_completed_date, &mut known);
        }
        append_fx_interval_writes(
            from_currency,
            interval,
            observations,
            latest_completed_date,
            !predecessor_fetch_failed,
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
pub(super) struct DateInterval {
    pub(super) start: NaiveDate,
    pub(super) end: NaiveDate,
}

#[derive(Clone, Copy)]
struct AssetFillScope {
    required_end: NaiveDate,
    cache_end: NaiveDate,
    source_end: NaiveDate,
    discover_boundary: bool,
}

pub(super) fn missing_intervals(
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

#[allow(clippy::too_many_arguments)]
fn append_asset_interval_writes(
    asset_id: i32,
    interval: DateInterval,
    observations: Vec<SourceObservation>,
    latest_completed_date: NaiveDate,
    persist_end: NaiveDate,
    observation_end: NaiveDate,
    allow_cached_predecessor: bool,
    known: &mut BTreeMap<NaiveDate, f64>,
    writes: &mut Vec<daily_price_repo::DailyPriceWrite>,
) {
    writes.extend(
        forward_filled_interval(
            interval,
            observations,
            latest_completed_date,
            observation_end,
            allow_cached_predecessor,
            known,
        )
        .into_iter()
        .filter(|(date, _)| *date <= persist_end)
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
    allow_cached_predecessor: bool,
    known: &mut BTreeMap<NaiveDate, f64>,
    writes: &mut Vec<exchange_rate_repo::ExchangeRateWrite>,
) {
    writes.extend(
        forward_filled_interval(
            interval,
            observations,
            latest_completed_date,
            interval.end,
            allow_cached_predecessor,
            known,
        )
        .into_iter()
        .map(|(date, value)| exchange_rate_repo::ExchangeRateWrite {
            from_currency: from_currency.to_owned(),
            to_currency: BASE_CURRENCY.to_owned(),
            date: format_date(date),
            rate: value,
        }),
    );
}

pub(super) fn forward_filled_interval(
    interval: DateInterval,
    observations: Vec<SourceObservation>,
    latest_completed_date: NaiveDate,
    observation_end: NaiveDate,
    allow_cached_predecessor: bool,
    known: &mut BTreeMap<NaiveDate, f64>,
) -> Vec<(NaiveDate, f64)> {
    let source_values =
        interval_source_values(observations, latest_completed_date, observation_end);
    let has_later_cached_value = known
        .range((interval.end + chrono::Duration::days(1))..)
        .next()
        .is_some();
    let fill_end = if has_later_cached_value {
        interval.end
    } else if let Some(source_end) = source_values.keys().next_back().copied() {
        source_end.min(interval.end)
    } else {
        return Vec::new();
    };
    let cached_predecessor = allow_cached_predecessor
        .then(|| known.range(..interval.start).next_back())
        .flatten();
    let source_predecessor = source_values.range(..interval.start).next_back();
    let mut last_known = match (cached_predecessor, source_predecessor) {
        (Some((cached_date, cached_value)), Some((source_date, source_value)))
            if source_date > cached_date =>
        {
            Some(*source_value)
        }
        (Some((_, cached_value)), _) => Some(*cached_value),
        (None, Some((_, source_value))) => Some(*source_value),
        (None, None) => None,
    };
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
    observations: Vec<SourceObservation>,
    latest_completed_date: NaiveDate,
    observation_end: NaiveDate,
) -> BTreeMap<NaiveDate, f64> {
    observations
        .into_iter()
        .filter(|observation| {
            observation.date <= observation_end && observation.date <= latest_completed_date
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

async fn fetch_asset_price_history_for_interval(
    market_data: &MarketData,
    asset: &Asset,
    lookup_identifier: &str,
    source_start: NaiveDate,
    interval: DateInterval,
    scope: AssetFillScope,
) -> anyhow::Result<(Vec<SourceObservation>, bool)> {
    let mut observations = fetch_asset_price_history(
        market_data,
        asset,
        lookup_identifier,
        source_start,
        interval.end,
    )
    .await?;
    let mut predecessor_fetch_failed = false;
    if scope.discover_boundary
        && !observations
            .iter()
            .any(|observation| observation.date < interval.start)
    {
        let predecessor = match asset.asset_type {
            AssetType::Stock => {
                market_data
                    .latest_stock_price_before(lookup_identifier, interval.start)
                    .await
            }
            AssetType::Fund | AssetType::Etf => {
                market_data
                    .latest_fund_price_before(lookup_identifier, interval.start)
                    .await
            }
        };
        match predecessor {
            Ok(Some(observation)) => observations.push(observation),
            Ok(None) => {}
            Err(error) => {
                predecessor_fetch_failed = true;
                tracing::warn!(
                    ticker = %asset.ticker,
                    error = %format!("{error:#}"),
                    "failed to fetch historical predecessor price"
                );
            }
        }
    }
    if !scope.discover_boundary
        || interval.end != scope.required_end
        || observations
            .iter()
            .any(|observation| observation.date >= interval.end)
    {
        return Ok((observations, predecessor_fetch_failed));
    }

    let boundary_start = interval.end + chrono::Duration::days(1);
    if boundary_start > scope.source_end {
        return Ok((observations, predecessor_fetch_failed));
    }
    match fetch_asset_price_history(
        market_data,
        asset,
        lookup_identifier,
        boundary_start,
        scope.source_end,
    )
    .await
    {
        Ok(boundary_observations) => observations.extend(boundary_observations),
        Err(error) => tracing::warn!(
            ticker = %asset.ticker,
            error = %format!("{error:#}"),
            "failed to fetch historical boundary prices"
        ),
    }
    Ok((observations, predecessor_fetch_failed))
}

fn preserve_source_predecessor(
    observations: &[SourceObservation],
    interval: DateInterval,
    latest_completed_date: NaiveDate,
    known: &mut BTreeMap<NaiveDate, f64>,
) {
    let Some((source_date, source_value)) =
        interval_source_values(observations.to_vec(), latest_completed_date, interval.end)
            .range(..interval.start)
            .next_back()
            .map(|(date, value)| (*date, *value))
    else {
        return;
    };
    let cached_date = known
        .range(..interval.start)
        .next_back()
        .map(|(date, _)| *date);
    if cached_date.is_none_or(|date| source_date > date) {
        known.insert(source_date, source_value);
    }
}
