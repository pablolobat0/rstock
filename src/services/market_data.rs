use std::collections::HashMap;

use anyhow::{bail, Context};
use chrono::{Datelike, NaiveDate, Weekday};
use sea_orm::DatabaseConnection;

use crate::constants::{display_date, format_date, BASE_CURRENCY, DATE_FORMAT};
use crate::db::repos::{asset_repo, daily_price_repo, exchange_rate_repo};
use crate::models::{
    Asset, AssetClassification, AssetDisplayMarketData, AssetType, BenchmarkMarketData,
    MarketDataLimitation, MarketDataLimitationClassification, MarketDataLimitationSource,
    MarketDataSubject, MarketDataValuation, NavMarketData,
};
use crate::services::metrics;
use crate::services::price::PriceFetcher;

struct LatestMarketDataDate {
    date: String,
    source: MarketDataLimitationSource,
}

const STALE_COMPLETED_WEEKDAY_WARNING_THRESHOLD: u32 = 4;

enum HistoricalMarketDataPurpose {
    Nav,
    Benchmark,
}

impl HistoricalMarketDataPurpose {
    fn error_prefix(&self) -> &'static str {
        match self {
            Self::Nav => "NAV",
            Self::Benchmark => "benchmark",
        }
    }
}

pub async fn prepare_nav_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    currency_pairs: &[String],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<NavMarketData> {
    prepare_historical_market_data(
        db,
        assets,
        currency_pairs,
        start_date,
        end_date,
        price_fetcher,
        HistoricalMarketDataPurpose::Nav,
    )
    .await
}

pub async fn prepare_benchmark_market_data(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<BenchmarkMarketData> {
    let info = metrics::benchmark_asset_info();
    let asset_id = match asset_repo::find_by_ticker(db, &info.ticker).await? {
        Some(asset) => asset.id,
        None => asset_repo::create(db, &info, &AssetClassification::default(), None).await?,
    };
    let benchmark = metrics::benchmark_asset(asset_id);
    let currency_pairs = if benchmark.currency == BASE_CURRENCY {
        Vec::new()
    } else {
        vec![currency_pair(&benchmark.currency)]
    };

    let market_data = prepare_historical_market_data(
        db,
        std::slice::from_ref(&benchmark),
        &currency_pairs,
        start_date,
        end_date,
        price_fetcher,
        HistoricalMarketDataPurpose::Benchmark,
    )
    .await?;

    Ok(BenchmarkMarketData {
        asset_id,
        effective_end: market_data.effective_end,
        limitations: market_data.limitations,
    })
}

pub fn currency_pair(from: &str) -> String {
    format!("{from}{BASE_CURRENCY}")
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

async fn prepare_historical_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    currency_pairs: &[String],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
    purpose: HistoricalMarketDataPurpose,
) -> anyhow::Result<NavMarketData> {
    let requested_end = parse_date(end_date, "historical market data end date")?;
    let latest_asset_dates =
        fill_nav_asset_prices(db, assets, start_date, end_date, price_fetcher).await?;
    let latest_rate_dates = if currency_pairs.is_empty() {
        HashMap::new()
    } else {
        fill_nav_exchange_rates(db, currency_pairs, start_date, end_date, price_fetcher).await?
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
        let latest_available_date = parse_date(&latest_date.date, "asset price date")?;
        latest_required_dates.push(latest_available_date);
        if let Some(limitation) = classify_asset_limitation(
            asset,
            latest_available_date,
            requested_end,
            latest_date.source.clone(),
        ) {
            limitations.push(limitation);
        }
    }

    for pair in currency_pairs {
        let Some(latest_date) = latest_rate_dates.get(pair) else {
            bail!("missing required historical market data for FX rate {pair}");
        };
        let latest_available_date = parse_date(&latest_date.date, "FX rate date")?;
        latest_required_dates.push(latest_available_date);
        if let Some(limitation) = classify_fx_limitation(
            pair,
            latest_available_date,
            requested_end,
            latest_date.source.clone(),
        ) {
            limitations.push(limitation);
        }
    }

    let effective_end = latest_required_dates.into_iter().min().with_context(|| {
        format!(
            "{} market data preparation had no date requirements",
            purpose.error_prefix()
        )
    })?;

    Ok(NavMarketData {
        effective_end,
        limitations,
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
    let fx_rate = get_asset_exchange_rate(db, asset, date).await?;

    Ok(Some(MarketDataValuation {
        native_price,
        fx_rate,
        base_currency_price: native_price * fx_rate,
    }))
}

pub async fn get_asset_display_market_data(
    db: &DatabaseConnection,
    asset: &Asset,
    fallback_native_price: f64,
    fallback_price_date: &str,
    fallback_fx_rate: f64,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<AssetDisplayMarketData> {
    let today = chrono::Local::now().date_naive();
    let today_str = crate::constants::format_date(today);
    let yesterday_str = crate::constants::format_date(today - chrono::Duration::days(1));

    let (native_price, price_date, price_limitation) = get_display_price(
        db,
        asset,
        &today_str,
        &yesterday_str,
        fallback_native_price,
        fallback_price_date,
        price_fetcher,
    )
    .await?;
    let (fx_rate, mut limitations) = get_display_exchange_rate(
        db,
        asset,
        &today_str,
        &yesterday_str,
        fallback_fx_rate,
        price_fetcher,
    )
    .await?;
    if let Some(limitation) = price_limitation {
        limitations.push(limitation);
    }

    Ok(AssetDisplayMarketData {
        native_price,
        price_date,
        fx_rate,
        base_currency_price: native_price * fx_rate,
        limitations,
    })
}

#[allow(clippy::implicit_hasher)]
pub fn get_asset_exchange_rate_from_prepared_rates(
    asset: &Asset,
    day_rates: &HashMap<String, f64>,
) -> anyhow::Result<f64> {
    if asset.currency == BASE_CURRENCY {
        return Ok(1.0);
    }

    let pair = currency_pair(&asset.currency);
    day_rates
        .get(&pair)
        .copied()
        .with_context(|| format!("missing required historical market data for FX rate {pair}"))
}

async fn get_asset_exchange_rate(
    db: &DatabaseConnection,
    asset: &Asset,
    date: &str,
) -> anyhow::Result<f64> {
    if asset.currency == BASE_CURRENCY {
        return Ok(1.0);
    }

    let pair = currency_pair(&asset.currency);
    get_exchange_rate(db, &pair, date)
        .await?
        .with_context(|| format!("missing required historical market data for FX rate {pair}"))
}

async fn get_display_price(
    db: &DatabaseConnection,
    asset: &Asset,
    today: &str,
    yesterday: &str,
    fallback_native_price: f64,
    fallback_price_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<(f64, String, Option<MarketDataLimitation>)> {
    if asset.asset_type == AssetType::Stock {
        if let Some(live_price) = fetch_live_asset_price(asset, today, price_fetcher)
            .await
            .unwrap_or(None)
        {
            return Ok((live_price, today.to_owned(), None));
        }
    }

    let (price, date) =
        match daily_price_repo::find_price_and_date_at_or_before(db, asset.id, yesterday).await? {
            Some((price, date)) => (price, date),
            None => (fallback_native_price, fallback_price_date.to_owned()),
        };
    let available_on = parse_date(&date, "asset price date")?;
    let requested_end = parse_date(yesterday, "display asset price end date")?;
    let limitation = classify_asset_limitation(
        asset,
        available_on,
        requested_end,
        MarketDataLimitationSource::CachedFallback,
    );

    Ok((price, date, limitation))
}

pub fn user_facing_market_data_warning(limitation: &MarketDataLimitation) -> Option<String> {
    if limitation.classification == MarketDataLimitationClassification::AcceptableReportingLag {
        return None;
    }

    let latest_available_date = display_date(&crate::constants::format_date(
        limitation.latest_available_date,
    ));
    let requested_end_date = display_date(&crate::constants::format_date(
        limitation.requested_end_date,
    ));
    let source = match limitation.source {
        MarketDataLimitationSource::CachedFallback => "using cached data",
        MarketDataLimitationSource::SourceLag => "source data is delayed",
    };

    Some(match &limitation.subject {
        MarketDataSubject::Asset {
            ticker,
            name,
            asset_type,
        } => format!(
            "Market data limitation: {asset_type} {ticker} ({name}) has latest price from {latest_available_date}; requested through {requested_end_date}; {source}."
        ),
        MarketDataSubject::FxRate { pair } => format!(
            "Market data limitation: FX rate {pair} has latest rate from {latest_available_date}; requested through {requested_end_date}; {source}."
        ),
    })
}

async fn get_display_exchange_rate(
    db: &DatabaseConnection,
    asset: &Asset,
    today: &str,
    yesterday: &str,
    fallback_fx_rate: f64,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<(f64, Vec<MarketDataLimitation>)> {
    if asset.currency == BASE_CURRENCY {
        return Ok((1.0, Vec::new()));
    }

    let pair = currency_pair(&asset.currency);
    if let Some(live_rate) = fetch_live_exchange_rate(&pair, today, price_fetcher)
        .await
        .unwrap_or(None)
    {
        return Ok((live_rate, Vec::new()));
    }

    let Some((rate, date_string)) =
        exchange_rate_repo::find_rate_and_date_at_or_before(db, &pair, yesterday).await?
    else {
        return Ok((fallback_fx_rate, Vec::new()));
    };
    let available_on = parse_date(&date_string, "FX rate date")?;
    let requested_end = parse_date(yesterday, "display FX end date")?;
    let limitations = classify_fx_limitation(
        &pair,
        available_on,
        requested_end,
        MarketDataLimitationSource::CachedFallback,
    )
    .into_iter()
    .collect();

    Ok((rate, limitations))
}

async fn fetch_live_asset_price(
    asset: &Asset,
    date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<f64>> {
    let lookup = match asset.asset_type {
        AssetType::Stock => asset.ticker.as_str(),
        AssetType::Fund | AssetType::Etf => {
            let Some(code) = asset.morningstar_code.as_deref() else {
                tracing::warn!(
                    ticker = %asset.ticker,
                    "skipping price fetch: fund/ETF has no morningstar_code set"
                );
                return Ok(None);
            };
            code
        }
    };
    let prices = price_fetcher
        .get_historical_prices(lookup, date, date, &asset.asset_type)
        .await?;
    Ok(prices.last().map(|(_, price)| *price))
}

async fn fetch_live_exchange_rate(
    pair: &str,
    date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<f64>> {
    let rates = price_fetcher
        .get_historical_exchange_rates(pair, date, date)
        .await?;
    Ok(rates.last().map(|(_, rate)| *rate))
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
                latest_dates.insert(
                    asset.id,
                    LatestMarketDataDate {
                        date,
                        source: MarketDataLimitationSource::SourceLag,
                    },
                );
            }
            Ok(None) => {
                tracing::warn!(ticker = %asset.ticker, "no new price data from API, falling back to latest cached date");
                if let Some(cached) = daily_price_repo::find_latest_date(db, asset.id).await? {
                    latest_dates.insert(
                        asset.id,
                        LatestMarketDataDate {
                            date: cached,
                            source: MarketDataLimitationSource::CachedFallback,
                        },
                    );
                } else {
                    tracing::warn!(ticker = %asset.ticker, "no cached price data available at all");
                }
            }
            Err(e) => {
                tracing::warn!(ticker = %asset.ticker, error = %e, "failed to fill prices, falling back to latest cached date");
                if let Some(cached) = daily_price_repo::find_latest_date(db, asset.id).await? {
                    latest_dates.insert(
                        asset.id,
                        LatestMarketDataDate {
                            date: cached,
                            source: MarketDataLimitationSource::CachedFallback,
                        },
                    );
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

    let requested_end = parse_date(end_date, "historical asset price end date")?;
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

    let start = parse_date(start_date, "historical asset price start date")?;
    let fill_end = parse_date(&last_api_date, "last historical asset price API date")?;
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

    let requested_end = parse_date(end_date, "historical FX end date")?;
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

    let start = parse_date(start_date, "historical FX start date")?;
    let fill_end = parse_date(&last_api_date, "last historical FX API date")?;
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
                latest_dates.insert(
                    pair.clone(),
                    LatestMarketDataDate {
                        date,
                        source: MarketDataLimitationSource::SourceLag,
                    },
                );
            }
            Ok(None) => {
                tracing::warn!(%pair, "no new exchange rate data from API, falling back to latest cached date");
                if let Some(cached) = exchange_rate_repo::find_latest_date(db, pair).await? {
                    latest_dates.insert(
                        pair.clone(),
                        LatestMarketDataDate {
                            date: cached,
                            source: MarketDataLimitationSource::CachedFallback,
                        },
                    );
                } else {
                    tracing::warn!(%pair, "no cached exchange rate data available at all");
                }
            }
            Err(e) => {
                tracing::warn!(%pair, error = %e, "failed to fill exchange rates, falling back to latest cached date");
                if let Some(cached) = exchange_rate_repo::find_latest_date(db, pair).await? {
                    latest_dates.insert(
                        pair.clone(),
                        LatestMarketDataDate {
                            date: cached,
                            source: MarketDataLimitationSource::CachedFallback,
                        },
                    );
                }
            }
        }
    }

    Ok(latest_dates)
}

fn classify_asset_limitation(
    asset: &Asset,
    latest_date: NaiveDate,
    requested_end: NaiveDate,
    source: MarketDataLimitationSource,
) -> Option<MarketDataLimitation> {
    if latest_date >= requested_end {
        return None;
    }

    let classification = match asset.asset_type {
        AssetType::Fund | AssetType::Etf => {
            if (requested_end - latest_date).num_days() <= 7 {
                MarketDataLimitationClassification::AcceptableReportingLag
            } else {
                MarketDataLimitationClassification::ActionableReportingLag
            }
        }
        AssetType::Stock => {
            if completed_weekdays_between(latest_date, requested_end)
                < STALE_COMPLETED_WEEKDAY_WARNING_THRESHOLD
            {
                return None;
            }
            MarketDataLimitationClassification::ActionableStaleData
        }
    };

    Some(MarketDataLimitation {
        subject: MarketDataSubject::Asset {
            ticker: asset.ticker.clone(),
            name: asset.name.clone(),
            asset_type: asset.asset_type.clone(),
        },
        latest_available_date: latest_date,
        requested_end_date: requested_end,
        classification,
        source,
    })
}

fn classify_fx_limitation(
    pair: &str,
    latest_date: NaiveDate,
    requested_end: NaiveDate,
    source: MarketDataLimitationSource,
) -> Option<MarketDataLimitation> {
    if latest_date >= requested_end
        || completed_weekdays_between(latest_date, requested_end)
            < STALE_COMPLETED_WEEKDAY_WARNING_THRESHOLD
    {
        return None;
    }

    Some(MarketDataLimitation {
        subject: MarketDataSubject::FxRate {
            pair: pair.to_owned(),
        },
        latest_available_date: latest_date,
        requested_end_date: requested_end,
        classification: MarketDataLimitationClassification::ActionableStaleData,
        source,
    })
}

fn completed_weekdays_between(latest_date: NaiveDate, requested_end: NaiveDate) -> u32 {
    let mut count = 0;
    let mut current = latest_date + chrono::Duration::days(1);
    while current <= requested_end {
        if !matches!(current.weekday(), Weekday::Sat | Weekday::Sun) {
            count += 1;
        }
        current += chrono::Duration::days(1);
    }
    count
}

fn parse_date(date: &str, label: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).with_context(|| format!("invalid {label}: {date}"))
}
