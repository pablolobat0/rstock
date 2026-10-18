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
    pub limitations: Vec<MarketDataLimitation>,
}

pub(crate) async fn prepare(
    db: &DatabaseConnection,
    reference_id: &str,
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<PreparedBenchmarkReference> {
    let reference = benchmark_reference_repo::find_by_id(db, reference_id).await?;
    let start = NaiveDate::parse_from_str(start_date, DATE_FORMAT)?;
    let end = NaiveDate::parse_from_str(end_date, DATE_FORMAT)?;
    let (known, mut limitations) = prepare_prices(
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
    for (date, rate) in &fx_values {
        anyhow::ensure!(
            rate.is_finite() && *rate > 0.0,
            "invalid persisted FX rate for {} on {}",
            reference.currency,
            date
        );
    }
    let series = known
        .range(start..=end)
        .filter_map(|(date, price)| {
            let rate = if reference.currency == BASE_CURRENCY {
                Some(1.0)
            } else {
                fx_values.get(date).copied()
            }?;
            let converted = price * rate;
            Some((format_date(*date), converted))
        })
        .collect::<Vec<_>>();
    for (date, converted) in &series {
        anyhow::ensure!(
            converted.is_finite() && *converted > 0.0,
            "invalid converted benchmark reference price for {} on {}: {}",
            reference.id,
            date,
            converted
        );
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
        limitations,
    })
}

async fn prepare_prices(
    db: &DatabaseConnection,
    reference: &benchmark_reference_repo::BenchmarkReference,
    start: NaiveDate,
    end: NaiveDate,
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<(BTreeMap<NaiveDate, f64>, Vec<MarketDataLimitation>)> {
    let price_rows =
        benchmark_reference_repo::find_prices_with_seed(db, &reference.id, start_date, end_date)
            .await?;
    let mut known = BTreeMap::new();
    let mut covered: HashSet<NaiveDate> = price_rows
        .iter()
        .filter(|row| !row.is_api_failure)
        .map(|row| NaiveDate::parse_from_str(&row.date, DATE_FORMAT))
        .collect::<Result<_, _>>()?;
    for row in price_rows.iter().filter(|row| !row.is_api_failure) {
        anyhow::ensure!(
            row.price.is_finite() && row.price > 0.0,
            "invalid persisted benchmark reference price for {} on {}: {}",
            reference.id,
            row.date,
            row.price
        );
        known.insert(
            NaiveDate::parse_from_str(&row.date, DATE_FORMAT)?,
            row.price,
        );
    }

    let completed_end = end.min(market_data.today() - Duration::days(1));
    for interval in historical::missing_intervals(start, completed_end, &covered) {
        let Ok(mut observations) = market_data
            .stock_price_history(&reference.source_identifier, interval.start, interval.end)
            .await
        else {
            continue;
        };
        if !observations.iter().any(|item| item.date < interval.start)
            && !known.keys().any(|date| *date < interval.start)
        {
            if let Ok(Some(predecessor)) = market_data
                .latest_stock_price_before(&reference.source_identifier, interval.start)
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
        }
        let filled = historical::forward_filled_interval(
            interval,
            observations,
            completed_end,
            interval.end,
            true,
            &mut known,
        );
        let writes: Vec<_> = filled
            .into_iter()
            .map(|(date, price)| {
                covered.insert(date);
                benchmark_reference_repo::ReferencePrice {
                    date: format_date(date),
                    price,
                    is_api_failure: false,
                }
            })
            .collect();
        benchmark_reference_repo::upsert_prices_immutable(db, &reference.id, &writes).await?;
    }

    let last_price = known.range(..=end).next_back();
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
    Ok((known, limitations))
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
