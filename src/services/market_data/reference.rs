use std::collections::{BTreeMap, HashSet};

use chrono::{Duration, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, BASE_CURRENCY, DATE_FORMAT};
use crate::db::repos::{benchmark_reference_repo, exchange_rate_repo};
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
    let series = known
        .range(start..=end)
        .filter_map(|(date, price)| {
            let rate = if reference.currency == BASE_CURRENCY {
                Some(1.0)
            } else {
                fx_values.range(..=*date).next_back().map(|(_, rate)| *rate)
            }?;
            Some((format_date(*date), price * rate))
        })
        .collect();
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
        known.insert(
            NaiveDate::parse_from_str(&row.date, DATE_FORMAT)?,
            row.price,
        );
    }

    let completed_end = end.min(market_data.today() - Duration::days(1));
    for (gap_start, gap_end) in missing_intervals(start, completed_end, &covered) {
        let Ok(mut observations) = market_data
            .stock_price_history(&reference.source_identifier, gap_start, gap_end)
            .await
        else {
            continue;
        };
        if !observations.iter().any(|item| item.date < gap_start)
            && !known.keys().any(|date| *date < gap_start)
        {
            if let Ok(Some(predecessor)) = market_data
                .latest_stock_price_before(&reference.source_identifier, gap_start)
                .await
            {
                observations.push(predecessor);
            }
        }
        observations.retain(|item| item.date <= completed_end && item.date <= gap_end);
        observations.sort_by_key(|item| item.date);
        let source_values: BTreeMap<NaiveDate, f64> = observations
            .into_iter()
            .map(|item| (item.date, item.value))
            .collect();
        let has_later_cached = covered.iter().any(|date| *date > gap_end);
        let fill_end = if has_later_cached {
            gap_end
        } else if let Some(last_source) = source_values.keys().next_back() {
            (*last_source).min(gap_end)
        } else {
            continue;
        };
        let mut last_value = source_values
            .range(..gap_start)
            .next_back()
            .map(|(_, value)| *value)
            .or_else(|| {
                known
                    .range(..gap_start)
                    .next_back()
                    .map(|(_, value)| *value)
            });
        let mut writes = Vec::new();
        let mut date = gap_start;
        while date <= fill_end {
            if let Some(value) = source_values.get(&date).copied().or(last_value) {
                writes.push(benchmark_reference_repo::ReferencePrice {
                    date: format_date(date),
                    price: value,
                    is_api_failure: false,
                });
                known.insert(date, value);
                covered.insert(date);
                last_value = Some(value);
            }
            date += Duration::days(1);
        }
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
    if currency != BASE_CURRENCY {
        historical::prepare_reference_exchange_rates(
            db,
            currency,
            start_date,
            end_date,
            market_data,
        )
        .await?;
    }
    let rows = exchange_rate_repo::find_rates_between_currencies(
        db,
        &[currency.to_owned()],
        BASE_CURRENCY,
        start_date,
        end_date,
    )
    .await?;
    let mut fx_values: BTreeMap<NaiveDate, f64> = rows
        .get(currency)
        .into_iter()
        .flatten()
        .map(|(date, rate)| Ok((NaiveDate::parse_from_str(date, DATE_FORMAT)?, *rate)))
        .collect::<Result<_, chrono::ParseError>>()?;
    if let Some((rate, date)) =
        exchange_rate_repo::find_rate_and_date_at_or_before(db, currency, BASE_CURRENCY, start_date)
            .await?
    {
        fx_values.insert(NaiveDate::parse_from_str(&date, DATE_FORMAT)?, rate);
    }
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

fn missing_intervals(
    start: NaiveDate,
    end: NaiveDate,
    covered: &HashSet<NaiveDate>,
) -> Vec<(NaiveDate, NaiveDate)> {
    if start > end {
        return Vec::new();
    }
    let mut gaps = Vec::new();
    let mut date = start;
    let mut gap_start = None;
    while date <= end {
        if covered.contains(&date) {
            if let Some(begin) = gap_start.take() {
                gaps.push((begin, date - Duration::days(1)));
            }
        } else if gap_start.is_none() {
            gap_start = Some(date);
        }
        date += Duration::days(1);
    }
    if let Some(begin) = gap_start {
        gaps.push((begin, end));
    }
    gaps
}
