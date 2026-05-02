use std::collections::HashMap;

use anyhow::{bail, Context};
use chrono::{Datelike, NaiveDate, Weekday};
use sea_orm::DatabaseConnection;

use crate::constants::DATE_FORMAT;
use crate::db::repos::{daily_price_repo, exchange_rate_repo};
use crate::models::{
    Asset, AssetType, MarketDataLimitation, MarketDataLimitationClassification,
    MarketDataLimitationSource, MarketDataSubject, NavMarketData,
};
use crate::services::daily_prices;
use crate::services::exchange_rates;
use crate::services::price::PriceFetcher;

struct LatestMarketDataDate {
    date: String,
    source: MarketDataLimitationSource,
}

pub async fn prepare_nav_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    currency_pairs: &[String],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<NavMarketData> {
    let requested_end = parse_date(end_date, "NAV end date")?;
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

    let effective_end = latest_required_dates
        .into_iter()
        .min()
        .context("NAV market data preparation had no date requirements")?;

    Ok(NavMarketData {
        effective_end,
        limitations,
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
            let result = daily_prices::fill_prices_for_range(
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
                exchange_rates::fill_rates_for_range(db, pair, start_date, end_date, price_fetcher)
                    .await;
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
            if completed_weekdays_between(latest_date, requested_end) == 0 {
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
    if latest_date >= requested_end || completed_weekdays_between(latest_date, requested_end) == 0 {
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
