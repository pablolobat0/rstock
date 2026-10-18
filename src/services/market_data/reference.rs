use std::collections::{BTreeMap, HashSet};

use chrono::{Duration, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, BASE_CURRENCY, DATE_FORMAT};
use crate::db::repos::benchmark_reference_repo;
use crate::models::{BenchmarkReferenceInfo, MarketDataLimitation};

use super::{historical, policy, MarketData};

pub(crate) struct PreparedBenchmarkReference {
    pub info: BenchmarkReferenceInfo,
    pub series: Vec<(String, f64)>,
    pub genuine_series: Vec<(String, f64)>,
    pub source_qualification: Option<crate::models::SeriesQualification>,
    pub limitations: Vec<MarketDataLimitation>,
}

struct CachedPriceSeries {
    known: BTreeMap<NaiveDate, f64>,
    genuine: BTreeMap<NaiveDate, f64>,
    covered: HashSet<NaiveDate>,
}

type ConvertedReferenceSeries = (Vec<(String, f64)>, Vec<(String, f64)>);

pub(crate) async fn prepare(
    db: &DatabaseConnection,
    reference_id: &str,
    start_date: &str,
    end_date: &str,
    require_requested_end: bool,
    market_data: &MarketData,
) -> anyhow::Result<PreparedBenchmarkReference> {
    let reference = benchmark_reference_repo::find_by_id(db, reference_id).await?;
    let source_qualification = match market_data
        .series_qualification(&reference.source, &reference.source_identifier)
        .await
    {
        Ok(qualification) => qualification,
        Err(error) => {
            tracing::warn!(source = reference.source, identifier = reference.source_identifier, error = %error, "benchmark reference qualification unavailable");
            None
        }
    };
    let start = NaiveDate::parse_from_str(start_date, DATE_FORMAT)?;
    let end = NaiveDate::parse_from_str(end_date, DATE_FORMAT)?;
    anyhow::ensure!(
        start <= end,
        "benchmark reference start date {start_date} is after end date {end_date}"
    );
    let (known, genuine, mut limitations) = prepare_prices(
        db,
        &reference,
        start,
        end,
        start_date,
        end_date,
        market_data,
    )
    .await?;
    let fx_values = prepare_exchange_rates(
        db,
        &reference.currency,
        start_date,
        end_date,
        end,
        market_data,
        &mut limitations,
    )
    .await?;
    let (series, genuine_series) =
        convert_reference_series(&reference, start, end, &known, &genuine, &fx_values)?;
    if require_requested_end && !series.iter().any(|(date, _)| date == end_date) {
        record_missing_requested_end(&reference, end, &known, &fx_values, &mut limitations);
    }
    Ok(PreparedBenchmarkReference {
        info: BenchmarkReferenceInfo {
            id: reference.id,
            display_name: reference.display_name,
            instrument_ticker: reference.instrument_ticker,
            source: reference.source,
            source_identifier: reference.source_identifier,
            currency: reference.currency,
            adjustment_provenance: reference.adjustment_provenance,
            return_basis_qualification: reference.return_basis_qualification,
        },
        series,
        genuine_series,
        source_qualification,
        limitations,
    })
}

fn convert_reference_series(
    reference: &benchmark_reference_repo::BenchmarkReference,
    start: NaiveDate,
    end: NaiveDate,
    known: &BTreeMap<NaiveDate, f64>,
    genuine: &BTreeMap<NaiveDate, f64>,
    fx_values: &BTreeMap<NaiveDate, f64>,
) -> anyhow::Result<ConvertedReferenceSeries> {
    for (date, rate) in fx_values {
        anyhow::ensure!(
            rate.is_finite() && *rate > 0.0,
            "invalid persisted FX rate for {} on {}",
            reference.currency,
            date
        );
    }
    let convert = |prices: &BTreeMap<NaiveDate, f64>| {
        prices
            .range(start..=end)
            .filter_map(|(date, price)| {
                let rate = if reference.currency == BASE_CURRENCY {
                    Some(1.0)
                } else {
                    fx_values.get(date).copied()
                }?;
                Some((format_date(*date), price * rate))
            })
            .collect::<Vec<_>>()
    };
    let series = convert(known);
    let genuine_series = convert(genuine);
    for (date, converted) in &series {
        anyhow::ensure!(
            converted.is_finite() && *converted > 0.0,
            "invalid converted benchmark reference price for {} on {}: {}",
            reference.id,
            date,
            converted
        );
    }
    Ok((series, genuine_series))
}

fn record_missing_requested_end(
    reference: &benchmark_reference_repo::BenchmarkReference,
    end: NaiveDate,
    known: &BTreeMap<NaiveDate, f64>,
    fx_values: &BTreeMap<NaiveDate, f64>,
    limitations: &mut Vec<MarketDataLimitation>,
) {
    let has_native_mark = known.contains_key(&end);
    let (subject, latest_available_date) = if reference.currency != BASE_CURRENCY && has_native_mark
    {
        (
            crate::models::MarketDataSubject::FxRate {
                currency: reference.currency.clone(),
            },
            fx_values.keys().next_back().copied(),
        )
    } else {
        (
            crate::models::MarketDataSubject::BenchmarkReference {
                id: reference.id.clone(),
                display_name: reference.display_name.clone(),
            },
            known.keys().next_back().copied(),
        )
    };
    limitations.push(MarketDataLimitation {
        subject,
        latest_available_date,
        requested_end_date: end,
        classification: crate::models::MarketDataLimitationClassification::ActionableMissingData,
    });
}

async fn prepare_prices(
    db: &DatabaseConnection,
    reference: &benchmark_reference_repo::BenchmarkReference,
    start: NaiveDate,
    end: NaiveDate,
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<(
    BTreeMap<NaiveDate, f64>,
    BTreeMap<NaiveDate, f64>,
    Vec<MarketDataLimitation>,
)> {
    let mut cached = load_cached_prices(db, &reference.id, start_date, end_date).await?;

    let completed_end = end.min(market_data.today() - Duration::days(1));
    for interval in historical::missing_intervals(start, completed_end, &cached.covered) {
        fetch_reference_interval(
            db,
            reference,
            interval,
            completed_end,
            &mut cached,
            market_data,
        )
        .await?;
    }

    let last_price = cached.known.range(..=end).next_back();
    let limitations = match last_price {
        Some((date, _)) => policy::classify_benchmark_reference_limitation(
            &reference.id,
            &reference.display_name,
            *date,
            end,
        )
        .into_iter()
        .collect(),
        None => vec![policy::missing_benchmark_reference_limitation(
            &reference.id,
            &reference.display_name,
            end,
        )],
    };
    Ok((cached.known, cached.genuine, limitations))
}

async fn load_cached_prices(
    db: &DatabaseConnection,
    reference_id: &str,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<CachedPriceSeries> {
    let rows =
        benchmark_reference_repo::find_prices_with_seed(db, reference_id, start_date, end_date)
            .await?;
    let mut known = BTreeMap::new();
    let mut genuine = BTreeMap::new();
    let mut covered = HashSet::new();
    for row in rows.iter().filter(|row| !row.is_api_failure) {
        let date = NaiveDate::parse_from_str(&row.date, DATE_FORMAT)?;
        anyhow::ensure!(
            row.price.is_finite() && row.price > 0.0,
            "invalid persisted benchmark reference price for {} on {}: {}",
            reference_id,
            row.date,
            row.price
        );
        known.insert(date, row.price);
        covered.insert(date);
        if row.is_source_observation {
            genuine.insert(date, row.price);
        }
    }
    Ok(CachedPriceSeries {
        known,
        genuine,
        covered,
    })
}

async fn fetch_reference_interval(
    db: &DatabaseConnection,
    reference: &benchmark_reference_repo::BenchmarkReference,
    interval: historical::DateInterval,
    completed_end: NaiveDate,
    cached: &mut CachedPriceSeries,
    market_data: &MarketData,
) -> anyhow::Result<()> {
    let Ok(mut observations) = market_data
        .benchmark_reference_price_history(
            &reference.source,
            &reference.source_identifier,
            interval.start,
            interval.end,
        )
        .await
    else {
        return Ok(());
    };
    if !observations.iter().any(|item| item.date < interval.start)
        && !cached.known.keys().any(|date| *date < interval.start)
    {
        if let Ok(Some(predecessor)) = market_data
            .latest_benchmark_reference_price_before(
                &reference.source,
                &reference.source_identifier,
                interval.start,
            )
            .await
        {
            observations.push(predecessor);
        }
    }
    observations.retain(|item| item.date <= completed_end && item.date <= interval.end);
    observations.sort_by_key(|item| item.date);
    for observation in &observations {
        anyhow::ensure!(
            observation.value.is_finite() && observation.value > 0.0,
            "invalid successful benchmark reference source price for {} on {}: {}",
            reference.id,
            observation.date,
            observation.value
        );
        cached.genuine.insert(observation.date, observation.value);
    }
    let filled = historical::forward_filled_interval(
        interval,
        observations,
        completed_end,
        interval.end,
        true,
        &mut cached.known,
    );
    let writes: Vec<_> = filled
        .into_iter()
        .map(|(date, price)| {
            cached.covered.insert(date);
            benchmark_reference_repo::ReferencePrice {
                date: format_date(date),
                price,
                is_api_failure: false,
                is_source_observation: cached.genuine.contains_key(&date),
            }
        })
        .collect();
    benchmark_reference_repo::upsert_prices_immutable(db, &reference.id, &writes).await?;
    Ok(())
}

async fn prepare_exchange_rates(
    db: &DatabaseConnection,
    currency: &str,
    start_date: &str,
    end_date: &str,
    end: NaiveDate,
    market_data: &MarketData,
    limitations: &mut Vec<MarketDataLimitation>,
) -> anyhow::Result<BTreeMap<NaiveDate, f64>> {
    let fx_values = if currency == BASE_CURRENCY {
        BTreeMap::new()
    } else {
        historical::prepare_reference_exchange_rates(
            db,
            currency,
            start_date,
            end_date,
            market_data,
        )
        .await?
    };
    if currency != BASE_CURRENCY {
        if let Some((date, _)) = fx_values.range(..=end).next_back() {
            if let Some(limitation) = policy::classify_fx_limitation(currency, *date, end) {
                limitations.push(limitation);
            }
        } else {
            limitations.push(policy::missing_fx_limitation(currency, end));
        }
    }
    Ok(fx_values)
}
