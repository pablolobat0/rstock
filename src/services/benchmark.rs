//! Shared eligibility and aligned-return calculation for benchmark comparisons.
//!
//! Market-data acquisition and EUR conversion remain in `MarketData`; this module
//! only admits compatible definitions and calculates presentation-neutral facts.

use crate::constants::format_date;
use crate::models::{
    ComparisonAvailability, ComparisonCoverageIssue, DatedWealthPoint, FundBenchmarkComparison,
};
use crate::services::market_data::MarketData;
use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate};
use sea_orm::DatabaseConnection;

pub type AlignedComparison = (Vec<DatedWealthPoint>, Vec<DatedWealthPoint>, f64, f64);
pub type ComparisonCalculation = anyhow::Result<Result<AlignedComparison, ComparisonCoverageIssue>>;

/// Resolve an explicit assignment, gate each source by its qualification,
/// prepare EUR series through `MarketData`, then calculate one complete outcome.
pub async fn compare_fund_to_primary(
    db: &DatabaseConnection,
    market_data: &MarketData,
    ms_code: &str,
    ticker: Option<&str>,
    fund_currency: Option<&str>,
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
    compare_assigned_fund(
        db,
        market_data,
        FundComparisonRequest {
            ms_code,
            ticker,
            fund_currency,
            today,
        },
        definition,
    )
    .await
}

struct FundComparisonRequest<'a> {
    ms_code: &'a str,
    ticker: &'a str,
    fund_currency: Option<&'a str>,
    today: NaiveDate,
}

async fn compare_assigned_fund(
    db: &DatabaseConnection,
    market_data: &MarketData,
    request: FundComparisonRequest<'_>,
    definition: crate::db::repos::benchmark_reference_repo::BenchmarkReference,
) -> anyhow::Result<FundBenchmarkComparison> {
    let mut outcome = FundBenchmarkComparison::unavailable("comparison inputs are not qualified");
    outcome.reference_id = Some(definition.id.clone());
    outcome.reference_name = Some(definition.display_name.clone());
    outcome.source = Some(definition.source.clone());
    outcome.source_identifier = Some(definition.source_identifier.clone());
    outcome.return_basis = Some(definition.return_basis_qualification.clone());
    outcome.adjustment = Some(definition.adjustment_qualification.clone());
    outcome.hedge_policy = Some(definition.hedge_policy.clone());
    outcome.native_currency = Some(definition.currency.clone());
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
    let end_date = format_date(request.today - Duration::days(1));
    let start = NaiveDate::from_ymd_opt(2000, 1, 1).expect("literal date should be valid");
    let subject_market = market_data
        .prepare_fund_comparison_market_data(
            db,
            crate::services::market_data::FundComparisonRequest {
                ms_code: request.ms_code,
                ticker: request.ticker,
                currency: request.fund_currency,
                start_date: start,
                end_date: request.today,
                requested_end_date: &end_date,
            },
        )
        .await?;
    outcome.fund_limitations = subject_market.limitations.clone();
    let Some(subject) = subject_market.qualification.as_ref() else {
        return Ok(unavailable(outcome, "fund source has no verified return-basis, adjustment, currency, hedge, and cadence qualification"));
    };
    let expected = (
        definition.currency.as_str(),
        definition.return_basis_qualification.as_str(),
        definition.adjustment_qualification.as_str(),
        definition.hedge_policy.as_str(),
    );
    if expected.0.is_empty()
        || expected.1.is_empty()
        || expected.2.is_empty()
        || expected.3.is_empty()
        || !matches_definition(subject, expected)
    {
        return Ok(unavailable(outcome, "fund or selected reference source does not match the selected definition currency, adjustment, return basis, or hedge policy; required convention is unknown or incompatible"));
    }
    if subject_market.completed_observation_count < 2 {
        outcome.cadence = Some(subject.cadence.clone());
        crate::constants::BASE_CURRENCY.clone_into(&mut outcome.currency);
        outcome.start_date = subject_market.first_observation_date.clone();
        outcome.end_date = subject_market.last_observation_date.clone();
        outcome.availability = ComparisonAvailability::NotApplicable {
            reason: "fund has fewer than two qualified completed observations; cumulative return is not measurable".to_owned(),
        };
        return Ok(outcome);
    }
    let Some(start_date) = subject_market.first_observation_date.as_deref() else {
        anyhow::bail!("fund comparison has completed observations but no first observation date");
    };
    let reference_qualification = market_data
        .selected_benchmark_reference_market_data(db, &definition.id, start_date, &end_date)
        .await?;
    outcome
        .reference_limitations
        .clone_from(&reference_qualification.limitations);
    let Some(reference) = reference_qualification.source_qualification.as_ref() else {
        return Ok(unavailable(outcome, "selected reference source has no verified return-basis, adjustment, currency, hedge, and cadence qualification"));
    };
    if !matches_definition(reference, expected) {
        return Ok(unavailable(
            outcome,
            "selected reference source does not match its explicit definition convention",
        ));
    }
    if subject.cadence != "daily" || reference.cadence != "daily" {
        return Ok(unavailable(
            outcome,
            "daily comparison requires verified daily observations; sparse cadence is not expanded",
        ));
    }
    outcome.cadence = Some(subject.cadence.clone());
    crate::constants::BASE_CURRENCY.clone_into(&mut outcome.currency);
    calculate_comparison(
        outcome,
        &subject_market,
        &reference_qualification,
        start_date,
        &end_date,
    )
}

fn calculate_comparison(
    mut outcome: FundBenchmarkComparison,
    subject_market: &crate::models::FundComparisonMarketData,
    reference_market: &crate::models::BenchmarkRiskMarketData,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<FundBenchmarkComparison> {
    if !outcome.fund_limitations.is_empty() || !outcome.reference_limitations.is_empty() {
        return Ok(unavailable(outcome, "required EUR comparison coverage is incomplete; see scoped fund and reference limitations"));
    }
    if reference_market.benchmark_series.len() < 2 {
        return Ok(unavailable(
            outcome,
            "primary reference has fewer than two qualified completed observations",
        ));
    }
    match compare_aligned_eur_series(
        &subject_market.eur_series,
        &reference_market.genuine_benchmark_series,
        start_date,
        end_date,
    )? {
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
        Err(reason) => Ok(unavailable(outcome, reason.to_string())),
    }
}

/// Aligns genuinely common observations across the declared complete interval.
/// Expected coverage gaps are data outcomes; malformed series remain errors.
pub fn compare_aligned_eur_series(
    fund: &[(String, f64)],
    reference: &[(String, f64)],
    requested_start: &str,
    requested_end: &str,
) -> ComparisonCalculation {
    let fund = validate_series(fund, "fund")?;
    let reference = validate_series(reference, "reference")?;
    let start = chrono::NaiveDate::parse_from_str(requested_start, "%Y-%m-%d")?;
    let end = chrono::NaiveDate::parse_from_str(requested_end, "%Y-%m-%d")?;
    anyhow::ensure!(start <= end, "comparison start date is after end date");
    if !fund.contains_key(requested_start) || !reference.contains_key(requested_start) {
        return Ok(Err(ComparisonCoverageIssue::MissingAnchor(
            requested_start.to_owned(),
        )));
    }
    if !fund.contains_key(requested_end) || !reference.contains_key(requested_end) {
        return Ok(Err(ComparisonCoverageIssue::MissingEnd(
            requested_end.to_owned(),
        )));
    }
    let mut day = start;
    while day <= end {
        let date = day.format("%Y-%m-%d").to_string();
        if !matches!(day.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
            && (!fund.contains_key(&date) || !reference.contains_key(&date))
        {
            return Ok(Err(ComparisonCoverageIssue::InternalWeekdayGap(date)));
        }
        day += chrono::Duration::days(1);
    }
    let aligned: Vec<_> = fund
        .iter()
        .filter_map(|(date, value)| reference.get(date).map(|other| (date, *value, *other)))
        .filter(|(date, _, _)| date.as_str() >= requested_start && date.as_str() <= requested_end)
        .collect();
    if aligned.len() < 2 {
        return Ok(Err(ComparisonCoverageIssue::InsufficientCommonObservations));
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
    Ok(Ok((
        fund_wealth,
        reference_wealth,
        (last.1 / first.1 - 1.0) * 100.0,
        (last.2 / first.2 - 1.0) * 100.0,
    )))
}

fn matches_definition(
    qualification: &crate::models::SeriesQualification,
    expected: (&str, &str, &str, &str),
) -> bool {
    qualification.currency == expected.0
        && qualification.return_basis == expected.1
        && qualification.adjustment == expected.2
        && qualification.hedge_policy == expected.3
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

fn validate_series(
    series: &[(String, f64)],
    label: &str,
) -> anyhow::Result<std::collections::BTreeMap<String, f64>> {
    let mut values = std::collections::BTreeMap::new();
    for (date, value) in series {
        chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .with_context(|| format!("malformed {label} observation date '{date}'"))?;
        if !value.is_finite() || *value <= 0.0 {
            anyhow::bail!("invalid {label} EUR observation on {date}");
        }
        if values.insert(date.clone(), *value).is_some() {
            anyhow::bail!("duplicate {label} observation on {date}");
        }
    }
    Ok(values)
}
