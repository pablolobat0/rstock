//! Shared eligibility and aligned-return calculation for benchmark comparisons.
//!
//! Market-data acquisition and EUR conversion remain in `MarketData`; this module
//! only admits compatible definitions and calculates presentation-neutral facts.

use crate::constants::format_date;
use crate::models::{ComparisonAvailability, DatedWealthPoint, FundBenchmarkComparison};
use crate::services::market_data::MarketData;
use chrono::{Duration, NaiveDate};
use sea_orm::DatabaseConnection;

/// Resolve an explicit assignment, gate each source by its qualification,
/// prepare EUR series through `MarketData`, then calculate one complete outcome.
pub async fn compare_fund_to_primary(
    db: &DatabaseConnection,
    market_data: &MarketData,
    ms_code: &str,
    ticker: Option<&str>,
    fund_prices: &[(String, f64)],
    today: NaiveDate,
) -> anyhow::Result<FundBenchmarkComparison> {
    let Some(ticker) = ticker else {
        return Ok(FundBenchmarkComparison::unavailable(
            "fund analysis has no exact tracked-asset identity; assign a reference to its ISIN",
        ));
    };
    let Some(assignment) =
        crate::db::repos::benchmark_assignment_repo::find_by_subject(db, "tracked", ticker).await?
    else {
        return Ok(FundBenchmarkComparison::unavailable(
            "no explicit primary reference is assigned to this tracked fund",
        ));
    };
    let definition =
        crate::db::repos::benchmark_reference_repo::find_by_id(db, &assignment.reference_id)
            .await?;
    let mut outcome = FundBenchmarkComparison::unavailable("comparison inputs are not qualified");
    outcome.reference_id = Some(definition.id.clone());
    outcome.reference_name = Some(definition.display_name.clone());
    outcome.return_basis = Some(definition.return_basis_qualification.clone());
    outcome.source_provenance = Some(definition.source_provenance.clone());
    outcome.cadence = Some("unverified".to_owned());
    if definition.verification_state != "verified" {
        return Ok(unavailable(
            outcome,
            format!(
                "primary reference '{}' has unverified production mapping ({})",
                definition.display_name, definition.verification_state
            ),
        ));
    }
    let Some(subject) = source_qualification(market_data, "morningstar", ms_code).await else {
        return Ok(unavailable(outcome, "fund source has no verified return-basis, adjustment, currency, hedge, and cadence qualification"));
    };
    let Some(reference) = source_qualification(
        market_data,
        &definition.source,
        &definition.source_identifier,
    )
    .await
    else {
        return Ok(unavailable(outcome, "selected reference source has no verified return-basis, adjustment, currency, hedge, and cadence qualification"));
    };
    if subject.currency != definition.currency
        || reference.currency != definition.currency
        || subject.return_basis != reference.return_basis
        || subject.adjustment != reference.adjustment
        || subject.hedge_policy != reference.hedge_policy
    {
        return Ok(unavailable(outcome, "fund and primary reference have incompatible currency, adjustment, return basis, or hedge policy"));
    }
    if subject.cadence != "daily" || reference.cadence != "daily" {
        return Ok(unavailable(
            outcome,
            "daily comparison requires verified daily observations; sparse cadence is not expanded",
        ));
    }
    outcome.cadence = Some(subject.cadence.clone());
    outcome.currency = definition.currency.clone();
    let Some(start_date) = fund_prices.first().map(|(date, _)| date.as_str()) else {
        return Ok(unavailable(outcome, "fund history is empty"));
    };
    let end_date = format_date(today - Duration::days(1));
    let (fund_eur, fund_limitations) = market_data
        .prepare_base_currency_prices(db, &subject.currency, fund_prices, &end_date)
        .await?;
    let reference_market = market_data
        .selected_benchmark_reference_market_data(db, &definition.id, start_date, &end_date)
        .await?;
    if !fund_limitations.is_empty() || !reference_market.limitations.is_empty() {
        return Ok(unavailable(outcome, "required EUR comparison coverage is incomplete; see fund/reference market-data limitations"));
    }
    match compare_aligned_eur_series(&fund_eur, &reference_market.benchmark_series) {
        Ok((fund_wealth, reference_wealth, fund_return, reference_return)) => {
            outcome.start_date = fund_wealth.first().map(|point| point.date.clone());
            outcome.end_date = fund_wealth.last().map(|point| point.date.clone());
            outcome.fund_return_pct = Some(fund_return);
            outcome.reference_return_pct = Some(reference_return);
            outcome.excess_return_percentage_points = Some(fund_return - reference_return);
            outcome.fund_wealth = fund_wealth;
            outcome.reference_wealth = reference_wealth;
            outcome.availability = ComparisonAvailability::Available;
            Ok(outcome)
        }
        Err(reason) => Ok(unavailable(outcome, reason)),
    }
}

async fn source_qualification(
    market_data: &MarketData,
    source: &str,
    identifier: &str,
) -> Option<crate::services::market_data::SeriesQualification> {
    match market_data.series_qualification(source, identifier).await {
        Ok(qualification) => qualification,
        Err(error) => {
            tracing::warn!(source, identifier, error = %error, "benchmark source qualification unavailable");
            None
        }
    }
}

fn unavailable<T: Into<String>>(
    mut outcome: FundBenchmarkComparison,
    reason: T,
) -> FundBenchmarkComparison {
    outcome.availability = ComparisonAvailability::Unavailable {
        reason: reason.into(),
    };
    outcome
}

/// Aligns only genuinely common observations. Any internal weekday gap makes
/// the comparison unavailable instead of silently dropping an interval.
pub fn compare_aligned_eur_series(
    fund: &[(String, f64)],
    reference: &[(String, f64)],
) -> Result<(Vec<DatedWealthPoint>, Vec<DatedWealthPoint>, f64, f64), String> {
    let fund = validate_series(fund, "fund")?;
    let reference = validate_series(reference, "reference")?;
    let aligned: Vec<_> = fund
        .iter()
        .filter_map(|(date, value)| reference.get(date).map(|other| (date, *value, *other)))
        .collect();
    if aligned.len() < 2 {
        return Err("fund and primary reference need at least two common EUR observations".into());
    }
    for pair in aligned.windows(2) {
        let start = chrono::NaiveDate::parse_from_str(pair[0].0, "%Y-%m-%d")
            .map_err(|_| "malformed fund observation date".to_owned())?;
        let end = chrono::NaiveDate::parse_from_str(pair[1].0, "%Y-%m-%d")
            .map_err(|_| "malformed fund observation date".to_owned())?;
        let mut day = start + chrono::Duration::days(1);
        while day < end {
            if !matches!(day.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
                && (!fund.contains_key(&day.format("%Y-%m-%d").to_string())
                    || !reference.contains_key(&day.format("%Y-%m-%d").to_string()))
            {
                return Err(format!(
                    "required internal weekday observation missing on {}",
                    day.format("%Y-%m-%d")
                ));
            }
            day += chrono::Duration::days(1);
        }
    }
    let first = aligned[0];
    let last = aligned[aligned.len() - 1];
    let fund_wealth = aligned
        .iter()
        .map(|(date, value, _)| DatedWealthPoint {
            date: (*date).clone(),
            wealth: value / first.1 * 100.0,
        })
        .collect();
    let reference_wealth = aligned
        .iter()
        .map(|(date, _, value)| DatedWealthPoint {
            date: (*date).clone(),
            wealth: value / first.2 * 100.0,
        })
        .collect();
    Ok((
        fund_wealth,
        reference_wealth,
        (last.1 / first.1 - 1.0) * 100.0,
        (last.2 / first.2 - 1.0) * 100.0,
    ))
}

fn validate_series(
    series: &[(String, f64)],
    label: &str,
) -> Result<std::collections::BTreeMap<String, f64>, String> {
    let mut values = std::collections::BTreeMap::new();
    for (date, value) in series {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| format!("malformed {label} observation date '{date}'"))?;
        if !value.is_finite() || *value <= 0.0 {
            return Err(format!("invalid {label} EUR observation on {date}"));
        }
        if values.insert(date.clone(), *value).is_some() {
            return Err(format!("duplicate {label} observation on {date}"));
        }
    }
    Ok(values)
}

use chrono::Datelike;
