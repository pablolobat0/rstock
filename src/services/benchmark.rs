//! Shared eligibility and aligned-return calculation for benchmark comparisons.
//!
//! Market-data acquisition and EUR conversion remain in `MarketData`; this module
//! only admits compatible definitions and calculates presentation-neutral facts.

use crate::constants::{format_date, FLOAT_EPSILON, MIN_DATA_POINTS, TRADING_DAYS_PER_YEAR};
use crate::db::repos::benchmark_secondary_assignment_repo::SecondaryAssignment;
use crate::models::{
    BenchmarkComparisonPeriod, BenchmarkPeriod, ComparisonAvailability, ComparisonCoverageIssue,
    ComparisonFact, ComparisonRole, DatedWealthPoint, FundBenchmarkComparison, FundBenchmarkPeriod,
    ReferencePurposeInfo,
};
use crate::services::benchmark_references::{
    exact_class_subjects, AssignmentSubject, FundCandidateIdentity,
};
use crate::services::market_data::MarketData;
use anyhow::Context;
use chrono::{Datelike, Duration, Months, NaiveDate, Weekday};
use sea_orm::DatabaseConnection;

pub type AlignedComparison = (Vec<DatedWealthPoint>, Vec<DatedWealthPoint>, f64, f64);
pub type ComparisonCalculation = anyhow::Result<Result<AlignedComparison, ComparisonCoverageIssue>>;
type StandardPeriodObservations = Vec<(String, f64, f64)>;
type StandardPeriodWindow = anyhow::Result<Result<StandardPeriodObservations, String>>;

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
        return Ok(FundBenchmarkComparison::unavailable_for_role(
            ComparisonRole::Primary,
            "fund analysis has no exact tracked-asset identity; assign a reference to its ISIN",
        ));
    };
    let Some(assignment) = crate::db::repos::benchmark_assignment_repo::find_by_subject(
        db,
        crate::services::benchmark_references::TRACKED_SUBJECT_TYPE,
        ticker,
    )
    .await?
    else {
        return Ok(FundBenchmarkComparison::unavailable_for_role(
            ComparisonRole::Primary,
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
            ticker: Some(ticker),
            candidate: None,
            fund_currency,
            today,
        },
        definition,
        ComparisonRole::Primary,
    )
    .await
}

/// Compares the fund against every explicitly selected secondary reference.
///
/// Each secondary is an independent diagnostic comparison. A secondary whose
/// inputs are missing, unqualified, or unverified makes only that secondary
/// unavailable: the primary comparison and the portfolio-only facts keep their
/// own outcomes.
///
/// The secondary assignments of one exact class are read from every record of
/// that class — the Tracked asset and the Fund candidate of the same exact
/// ISIN and vehicle — because both describe the same investment. One
/// reference is reported once even when several records select it. An
/// untracked candidate needs no Tracked asset: its exact candidate identity
/// resolves the same secondary assignments.
pub async fn compare_fund_to_secondary_references(
    db: &DatabaseConnection,
    market_data: &MarketData,
    ms_code: &str,
    ticker: Option<&str>,
    candidate: Option<&FundCandidateIdentity>,
    fund_currency: Option<&str>,
    today: NaiveDate,
) -> anyhow::Result<Vec<FundBenchmarkComparison>> {
    let assignments = secondary_assignments_of_exact_class(db, ticker, candidate).await?;
    let mut comparisons = Vec::with_capacity(assignments.len());
    for assignment in assignments {
        let definition =
            crate::db::repos::benchmark_reference_repo::find_by_id(db, &assignment.reference_id)
                .await?;
        let comparison = compare_assigned_fund(
            db,
            market_data,
            FundComparisonRequest {
                ms_code,
                ticker,
                candidate,
                fund_currency,
                today,
            },
            definition,
            ComparisonRole::Secondary,
        )
        .await?;
        comparisons.push(comparison);
    }
    Ok(comparisons)
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

/// Computes the standard fund/reference windows independently of the graph
/// selection.
///
/// The fund comparison keeps its accepted output row; the shared calculation
/// behind it is [`compute_standard_benchmark_periods`].
pub fn compute_standard_fund_benchmark_periods(
    subject: &[(String, f64)],
    reference: &[(String, f64)],
    comparison_start: &str,
    comparison_end: &str,
) -> anyhow::Result<Vec<FundBenchmarkPeriod>> {
    Ok(
        compute_standard_benchmark_periods(subject, reference, comparison_start, comparison_end)?
            .into_iter()
            .map(FundBenchmarkPeriod::from)
            .collect(),
    )
}

/// Computes the standard subject/reference windows independently of the graph
/// selection. Trailing anchors use the latest common close on or before the
/// requested date so a weekend or market holiday never shortens the period;
/// the actual anchor is returned to callers.
pub fn compute_standard_benchmark_periods(
    subject: &[(String, f64)],
    reference: &[(String, f64)],
    comparison_start: &str,
    comparison_end: &str,
) -> anyhow::Result<Vec<BenchmarkPeriod>> {
    let subject = validate_series(subject, "fund")?;
    let reference = validate_series(reference, "reference")?;
    let comparison_start = NaiveDate::parse_from_str(comparison_start, "%Y-%m-%d")?;
    let comparison_end = NaiveDate::parse_from_str(comparison_end, "%Y-%m-%d")?;
    anyhow::ensure!(
        comparison_start <= comparison_end,
        "comparison start date is after end date"
    );

    let mut periods = Vec::with_capacity(5);
    for period in [
        BenchmarkComparisonPeriod::Ytd,
        BenchmarkComparisonPeriod::OneYear,
        BenchmarkComparisonPeriod::ThreeYear,
        BenchmarkComparisonPeriod::FiveYear,
        BenchmarkComparisonPeriod::SinceComparisonStart,
    ] {
        periods.push(compute_standard_period(
            period,
            &subject,
            &reference,
            comparison_start,
            comparison_end,
        )?);
    }
    Ok(periods)
}

struct FundComparisonRequest<'a> {
    ms_code: &'a str,
    ticker: Option<&'a str>,
    candidate: Option<&'a FundCandidateIdentity>,
    fund_currency: Option<&'a str>,
    today: NaiveDate,
}

/// The identity of the compared class, as it is shown in a limitation.
///
/// A Tracked class is named by its ticker and an untracked candidate class by
/// its exact ISIN, so a missing observation never hides behind an empty
/// identity.
fn subject_label(request: &FundComparisonRequest<'_>) -> String {
    match (request.ticker, request.candidate) {
        (Some(ticker), _) => ticker.to_owned(),
        (None, Some(candidate)) => candidate.identifier.clone(),
        (None, None) => request.ms_code.to_owned(),
    }
}

/// The secondary assignments of one exact class, without duplicates.
///
/// Every record of the exact class contributes its secondary selections: the
/// Tracked asset and the Fund candidate of the same exact ISIN and vehicle
/// describe the same investment. A reference selected by more than one record
/// is reported once.
async fn secondary_assignments_of_exact_class(
    db: &DatabaseConnection,
    ticker: Option<&str>,
    candidate: Option<&FundCandidateIdentity>,
) -> anyhow::Result<Vec<SecondaryAssignment>> {
    let mut subjects: Vec<AssignmentSubject> = Vec::new();
    if let Some(ticker) = ticker {
        subjects.push(AssignmentSubject::tracked(ticker));
    }
    if let Some(candidate) = candidate {
        subjects.push(AssignmentSubject::candidate(candidate));
    }
    let mut exact_class: Vec<AssignmentSubject> = Vec::new();
    for subject in &subjects {
        for equivalent in exact_class_subjects(db, subject).await? {
            if !exact_class.contains(&equivalent) {
                exact_class.push(equivalent);
            }
        }
    }
    let mut assignments: Vec<SecondaryAssignment> = Vec::new();
    for subject in exact_class {
        for secondary in crate::db::repos::benchmark_secondary_assignment_repo::find_by_subject(
            db,
            &subject.subject_type,
            &subject.subject_id,
        )
        .await?
        {
            if !assignments
                .iter()
                .any(|existing| existing.reference_id == secondary.reference_id)
            {
                assignments.push(secondary);
            }
        }
    }
    Ok(assignments)
}

async fn compare_assigned_fund(
    db: &DatabaseConnection,
    market_data: &MarketData,
    request: FundComparisonRequest<'_>,
    definition: crate::db::repos::benchmark_reference_repo::BenchmarkReference,
    role: ComparisonRole,
) -> anyhow::Result<FundBenchmarkComparison> {
    let mut outcome =
        FundBenchmarkComparison::unavailable_for_role(role, "comparison inputs are not qualified");
    outcome.purpose = Some(ReferencePurposeInfo::from(&definition));
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
                "{} reference '{}' has unverified production mapping ({})",
                role.as_str(),
                definition.display_name,
                definition.verification_state
            ),
        ));
    }
    let end_date = format_date(latest_completed_weekday(request.today));
    let start = NaiveDate::from_ymd_opt(2000, 1, 1).expect("literal date should be valid");
    let subject_label = subject_label(&request);
    let subject_market = market_data
        .prepare_fund_comparison_market_data(
            db,
            crate::services::market_data::FundComparisonRequest {
                ms_code: request.ms_code,
                ticker: subject_label.as_str(),
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
        return Ok(not_applicable_short_history(
            outcome,
            &subject_market,
            &subject.cadence,
        ));
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

fn latest_completed_weekday(today: NaiveDate) -> NaiveDate {
    let mut date = today - Duration::days(1);
    while matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        date -= Duration::days(1);
    }
    date
}

/// The purpose of one selected reference definition.
///
/// The definition is immutable, so the rendered comparison keeps the meaning
/// of its reference instead of only an identifier.
impl From<&crate::db::repos::benchmark_reference_repo::BenchmarkReference>
    for ReferencePurposeInfo
{
    fn from(definition: &crate::db::repos::benchmark_reference_repo::BenchmarkReference) -> Self {
        Self {
            role: definition.role.clone(),
            universe: definition.universe.clone(),
            instrument_ticker: definition.instrument_ticker.clone(),
            history_boundary: definition.history_boundary.clone(),
            verification_state: definition.verification_state.clone(),
            official_comparator: definition.official_comparator.clone(),
            objective_comparator: definition.objective_comparator.clone(),
        }
    }
}

fn not_applicable_short_history(
    mut outcome: FundBenchmarkComparison,
    subject_market: &crate::models::FundComparisonMarketData,
    cadence: &str,
) -> FundBenchmarkComparison {
    const REASON: &str =
        "fund has fewer than two qualified completed observations; cumulative return is not measurable";
    outcome.cadence = Some(cadence.to_owned());
    crate::constants::BASE_CURRENCY.clone_into(&mut outcome.currency);
    outcome
        .start_date
        .clone_from(&subject_market.first_observation_date);
    outcome
        .end_date
        .clone_from(&subject_market.last_observation_date);
    outcome.availability = ComparisonAvailability::NotApplicable {
        reason: REASON.to_owned(),
    };
    outcome.standard_periods = BenchmarkComparisonPeriod::standard_periods()
        .into_iter()
        .map(|period| {
            FundBenchmarkPeriod::not_applicable(
                period,
                "fund has fewer than two qualified completed observations",
            )
        })
        .collect();
    outcome
}

fn calculate_comparison(
    mut outcome: FundBenchmarkComparison,
    subject_market: &crate::models::FundComparisonMarketData,
    reference_market: &crate::models::BenchmarkRiskMarketData,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<FundBenchmarkComparison> {
    outcome.standard_periods = compute_standard_fund_benchmark_periods(
        &subject_market.eur_series,
        &reference_market.genuine_benchmark_series,
        start_date,
        end_date,
    )?;
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

fn compute_standard_period(
    period: BenchmarkComparisonPeriod,
    subject: &std::collections::BTreeMap<String, f64>,
    reference: &std::collections::BTreeMap<String, f64>,
    comparison_start: NaiveDate,
    comparison_end: NaiveDate,
) -> anyhow::Result<BenchmarkPeriod> {
    let requested_start =
        match period {
            BenchmarkComparisonPeriod::Ytd => NaiveDate::from_ymd_opt(comparison_end.year(), 1, 1)
                .expect("January 1 is always a valid date"),
            BenchmarkComparisonPeriod::OneYear => comparison_end
                .checked_sub_months(Months::new(12))
                .context("could not calculate one-year comparison anchor")?,
            BenchmarkComparisonPeriod::ThreeYear => comparison_end
                .checked_sub_months(Months::new(36))
                .context("could not calculate three-year comparison anchor")?,
            BenchmarkComparisonPeriod::FiveYear => comparison_end
                .checked_sub_months(Months::new(60))
                .context("could not calculate five-year comparison anchor")?,
            BenchmarkComparisonPeriod::SinceComparisonStart => comparison_start,
        };
    let requested_start_string = format_date(requested_start);
    let mut outcome =
        BenchmarkPeriod::unavailable(period, "period comparison has unavailable required inputs");
    outcome.requested_start_date = Some(requested_start_string.clone());
    let end_date = format_date(comparison_end);
    if !subject.contains_key(&end_date) || !reference.contains_key(&end_date) {
        outcome.end_date = Some(end_date);
        set_period_unavailable(
            &mut outcome,
            "required comparison end observation is missing",
        );
        return Ok(outcome);
    }

    let Some(actual_start) = select_actual_period_start(
        period,
        subject,
        reference,
        &requested_start_string,
        requested_start,
        comparison_start,
        &mut outcome,
    ) else {
        return Ok(outcome);
    };
    outcome.start_date = Some(actual_start.clone());
    outcome.end_date = Some(end_date.clone());

    let observation_dates = match aligned_weekday_observations(
        subject,
        reference,
        &actual_start,
        comparison_end,
        &end_date,
    )? {
        Ok(observations) => observations,
        Err(reason) => {
            set_period_unavailable(&mut outcome, &reason);
            return Ok(outcome);
        }
    };
    calculate_period_returns_and_risk(&mut outcome, &observation_dates)?;
    Ok(outcome)
}

/// Selects the anchor observation that opens a standard period window.
///
/// The year-to-date window opens at the subject's own anchor, the
/// since-comparison-start window opens at the approved comparison start, and
/// the trailing windows open at the latest common close on or before the
/// requested trailing date.
fn select_actual_period_start(
    period: BenchmarkComparisonPeriod,
    subject: &std::collections::BTreeMap<String, f64>,
    reference: &std::collections::BTreeMap<String, f64>,
    requested_start_string: &str,
    requested_start: NaiveDate,
    comparison_start: NaiveDate,
    outcome: &mut BenchmarkPeriod,
) -> Option<String> {
    match period {
        BenchmarkComparisonPeriod::Ytd => select_year_to_date_start(
            subject,
            reference,
            requested_start,
            comparison_start,
            outcome,
        ),
        BenchmarkComparisonPeriod::SinceComparisonStart => select_comparison_start_anchor(
            subject,
            reference,
            requested_start,
            requested_start_string,
            comparison_start,
            outcome,
        ),
        _ => select_trailing_start(
            subject,
            reference,
            requested_start_string,
            requested_start,
            comparison_start,
            outcome,
        ),
    }
}

/// The year-to-date window opens at the subject's own anchor: the first
/// weekday close the subject reports on or after the calendar-year start
/// bounded by the approved comparison start. A reference that does not report
/// that exact close cannot supply the window, so the period stays unavailable
/// instead of opening later on a shortened window.
fn select_year_to_date_start(
    subject: &std::collections::BTreeMap<String, f64>,
    reference: &std::collections::BTreeMap<String, f64>,
    year_start: NaiveDate,
    comparison_start: NaiveDate,
    outcome: &mut BenchmarkPeriod,
) -> Option<String> {
    let approved_start = year_start.max(comparison_start);
    let approved_start_string = format_date(approved_start);
    let anchor = subject
        .keys()
        .find(|date| date.as_str() >= approved_start_string.as_str())
        .filter(|date| is_weekday_observation(date.as_str()))
        .cloned();
    let Some(anchor) = anchor else {
        set_period_unavailable(
            outcome,
            "required subject observations are missing for the requested year to date period",
        );
        return None;
    };
    if !reference.contains_key(&anchor) {
        set_period_unavailable(
            outcome,
            "required common subject/reference anchor observation is missing",
        );
        return None;
    }
    Some(anchor)
}

/// The since-comparison-start window opens at the approved comparison start,
/// which both series must report on the same close.
fn select_comparison_start_anchor(
    subject: &std::collections::BTreeMap<String, f64>,
    reference: &std::collections::BTreeMap<String, f64>,
    requested_start: NaiveDate,
    requested_start_string: &str,
    comparison_start: NaiveDate,
    outcome: &mut BenchmarkPeriod,
) -> Option<String> {
    let comparison_start_string = format_date(comparison_start);
    let anchor = subject
        .keys()
        .find(|date| date.as_str() == comparison_start_string)
        .filter(|date| reference.contains_key(*date))
        .cloned();
    if let Some(anchor) = anchor {
        return Some(anchor);
    }
    classify_missing_period_anchor(
        outcome,
        subject,
        reference,
        requested_start,
        requested_start_string,
        comparison_start,
    );
    None
}

/// The trailing window opens at the latest common close on or before the
/// requested trailing date and never before the approved comparison start.
fn select_trailing_start(
    subject: &std::collections::BTreeMap<String, f64>,
    reference: &std::collections::BTreeMap<String, f64>,
    requested_start_string: &str,
    requested_start: NaiveDate,
    comparison_start: NaiveDate,
    outcome: &mut BenchmarkPeriod,
) -> Option<String> {
    let anchor = subject
        .keys()
        .filter(|date| reference.contains_key(*date))
        .filter(|date| date.as_str() <= requested_start_string)
        .filter(|date| {
            NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok_and(|date| date >= comparison_start)
        })
        .max()
        .cloned();
    if let Some(anchor) = anchor {
        return Some(anchor);
    }
    classify_missing_period_anchor(
        outcome,
        subject,
        reference,
        requested_start,
        requested_start_string,
        comparison_start,
    );
    None
}

fn classify_missing_period_anchor(
    outcome: &mut BenchmarkPeriod,
    subject: &std::collections::BTreeMap<String, f64>,
    reference: &std::collections::BTreeMap<String, f64>,
    requested_start: NaiveDate,
    requested_start_string: &str,
    comparison_start: NaiveDate,
) {
    if subject.is_empty() || reference.is_empty() {
        set_period_unavailable(
            outcome,
            "required subject or reference observations are missing",
        );
        return;
    }
    let subject_starts_after = subject
        .first_key_value()
        .is_some_and(|(date, _)| date.as_str() > requested_start_string);
    let reference_starts_after = reference
        .first_key_value()
        .is_some_and(|(date, _)| date.as_str() > requested_start_string);
    if requested_start < comparison_start || subject_starts_after || reference_starts_after {
        set_period_not_applicable(
            outcome,
            "insufficient comparison history for the full requested period",
        );
    } else {
        set_period_unavailable(
            outcome,
            "required common subject/reference anchor observation is missing",
        );
    }
}

fn aligned_weekday_observations(
    subject: &std::collections::BTreeMap<String, f64>,
    reference: &std::collections::BTreeMap<String, f64>,
    actual_start: &str,
    comparison_end: NaiveDate,
    end_date: &str,
) -> StandardPeriodWindow {
    let start_date = NaiveDate::parse_from_str(actual_start, "%Y-%m-%d")?;
    if start_date > comparison_end {
        return Ok(Err("period has no usable interval".to_owned()));
    }
    let mut observations = Vec::new();
    let mut date = start_date;
    while date <= comparison_end {
        if is_weekday(date) {
            let key = format_date(date);
            let (Some(subject_value), Some(reference_value)) =
                (subject.get(&key), reference.get(&key))
            else {
                return Ok(Err(format!(
                    "required internal weekday observation is missing on {key}"
                )));
            };
            observations.push((key, *subject_value, *reference_value));
        }
        date += Duration::days(1);
    }
    if observations.first().map(|point| point.0.as_str()) != Some(actual_start) {
        return Ok(Err(
            "comparison anchor is not a weekday observation".to_owned()
        ));
    }
    if observations.last().map(|point| point.0.as_str()) != Some(end_date) {
        return Ok(Err("comparison end is not a weekday observation".to_owned()));
    }
    Ok(Ok(observations))
}

fn calculate_period_returns_and_risk(
    outcome: &mut BenchmarkPeriod,
    observations: &[(String, f64, f64)],
) -> anyhow::Result<()> {
    let first = observations
        .first()
        .context("aligned comparison has no anchor observation")?;
    let last = observations
        .last()
        .context("aligned comparison has no ending observation")?;
    let fund_return = checked_return(first.1, last.1)?;
    let reference_return = checked_return(first.2, last.2)?;
    outcome.subject_return_pct =
        ComparisonFact::available(finite_metric(fund_return * 100.0, "subject return")?);
    outcome.reference_return_pct =
        ComparisonFact::available(finite_metric(reference_return * 100.0, "reference return")?);
    outcome.excess_return_percentage_points = ComparisonFact::available(finite_metric(
        (fund_return - reference_return) * 100.0,
        "excess return",
    )?);

    let aligned_log_returns: Vec<(f64, f64)> = observations
        .windows(2)
        .map(|interval| {
            (
                (interval[1].1 / interval[0].1).ln(),
                (interval[1].2 / interval[0].2).ln(),
            )
        })
        .collect();
    let active_returns: Vec<f64> = observations
        .windows(2)
        .map(|interval| {
            (interval[1].1 / interval[0].1 - 1.0) - (interval[1].2 / interval[0].2 - 1.0)
        })
        .collect();
    if aligned_log_returns
        .iter()
        .flat_map(|(subject, reference)| [subject, reference])
        .chain(active_returns.iter())
        .any(|value| !value.is_finite())
    {
        anyhow::bail!("nonfinite daily return in standard benchmark comparison");
    }
    if aligned_log_returns.len() < MIN_DATA_POINTS {
        for (fact, name) in [
            (&mut outcome.beta, "beta"),
            (&mut outcome.correlation, "correlation"),
            (&mut outcome.tracking_error_annualized_pct, "tracking error"),
            (&mut outcome.information_ratio, "information ratio"),
        ] {
            *fact = ComparisonFact::not_applicable(format!(
                "fewer than {MIN_DATA_POINTS} aligned daily observations for {name}"
            ));
        }
        return Ok(());
    }

    let subject_logs: Vec<f64> = aligned_log_returns.iter().map(|pair| pair.0).collect();
    let reference_logs: Vec<f64> = aligned_log_returns.iter().map(|pair| pair.1).collect();
    let subject_variance = sample_variance(&subject_logs);
    let reference_variance = sample_variance(&reference_logs);
    let covariance = sample_covariance(&subject_logs, &reference_logs);
    outcome.beta = if reference_variance == 0.0 {
        ComparisonFact::not_applicable("zero benchmark variance makes beta undefined")
    } else {
        ComparisonFact::available(finite_metric(covariance / reference_variance, "beta")?)
    };
    outcome.correlation = if subject_variance == 0.0 || reference_variance == 0.0 {
        ComparisonFact::not_applicable(
            "zero variance in a return series makes correlation undefined",
        )
    } else {
        ComparisonFact::available(finite_metric(
            (covariance / (subject_variance * reference_variance).sqrt()).clamp(-1.0, 1.0),
            "correlation",
        )?)
    };

    let active_mean = active_returns.iter().sum::<f64>() / active_returns.len() as f64;
    let active_variance = sample_variance(&active_returns);
    let active_std = active_variance.sqrt();
    outcome.tracking_error_annualized_pct = ComparisonFact::available(finite_metric(
        active_std * TRADING_DAYS_PER_YEAR.sqrt() * 100.0,
        "tracking error",
    )?);
    outcome.information_ratio = if active_std == 0.0 {
        ComparisonFact::not_applicable(
            "zero active-return dispersion makes information ratio undefined",
        )
    } else {
        ComparisonFact::available(finite_metric(
            active_mean / active_std * TRADING_DAYS_PER_YEAR.sqrt(),
            "information ratio",
        )?)
    };
    Ok(())
}

fn set_period_not_applicable(outcome: &mut BenchmarkPeriod, reason: &str) {
    outcome.subject_return_pct = ComparisonFact::not_applicable(reason);
    outcome.reference_return_pct = ComparisonFact::not_applicable(reason);
    outcome.excess_return_percentage_points = ComparisonFact::not_applicable(reason);
    outcome.beta = ComparisonFact::not_applicable(reason);
    outcome.correlation = ComparisonFact::not_applicable(reason);
    outcome.tracking_error_annualized_pct = ComparisonFact::not_applicable(reason);
    outcome.information_ratio = ComparisonFact::not_applicable(reason);
}

fn set_period_unavailable(outcome: &mut BenchmarkPeriod, reason: &str) {
    outcome.subject_return_pct = ComparisonFact::unavailable(reason);
    outcome.reference_return_pct = ComparisonFact::unavailable(reason);
    outcome.excess_return_percentage_points = ComparisonFact::unavailable(reason);
    outcome.beta = ComparisonFact::unavailable(reason);
    outcome.correlation = ComparisonFact::unavailable(reason);
    outcome.tracking_error_annualized_pct = ComparisonFact::unavailable(reason);
    outcome.information_ratio = ComparisonFact::unavailable(reason);
}

fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

fn is_weekday_observation(date: &str) -> bool {
    NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok_and(is_weekday)
}

fn checked_return(start: f64, end: f64) -> anyhow::Result<f64> {
    let value = end / start - 1.0;
    anyhow::ensure!(value.is_finite(), "nonfinite cumulative benchmark return");
    Ok(value)
}

fn sample_variance(values: &[f64]) -> f64 {
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    if variance <= FLOAT_EPSILON.powi(2) {
        0.0
    } else {
        variance
    }
}

fn sample_covariance(left: &[f64], right: &[f64]) -> f64 {
    let left_mean = left.iter().sum::<f64>() / left.len() as f64;
    let right_mean = right.iter().sum::<f64>() / right.len() as f64;
    left.iter()
        .zip(right)
        .map(|(left, right)| (left - left_mean) * (right - right_mean))
        .sum::<f64>()
        / (left.len() - 1) as f64
}

fn finite_metric(value: f64, name: &str) -> anyhow::Result<f64> {
    anyhow::ensure!(
        value.is_finite(),
        "nonfinite {name} in benchmark comparison"
    );
    Ok(value)
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
