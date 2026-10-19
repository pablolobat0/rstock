//! Exposure-matched portfolio benchmark.
//!
//! The comparison is the primary benchmark of the Portfolio dashboard. It
//! combines the explicitly assigned primary reference of every held asset
//! through the EUR exposures of the *preceding* Complete NAV snapshot, so
//! the weights that measured a past interval are the exposures that were
//! actually held at the previous close. Today's holdings are never projected
//! backwards, and a same-day trade only changes the next interval's weights.
//!
//! The complete NAV scope is the denominator of every weight. Monetary
//! holdings are excluded, and the retained Net dividend income inside NAV is
//! carried as a 0%-return residual. Several assets assigned the same primary
//! reference aggregate their weights into one contribution instead of being
//! counted twice, and secondary references never enter the weights. A missing
//! assignment or a missing required component makes the comparison
//! unavailable instead of renormalizing the remaining weights.
//!
//! The reference series uses the *source-reported* observations only. A
//! forward-filled row is not an independent mark, so a qualified reference
//! observation must exist on the comparison anchor, on the comparison end, and
//! on every completed weekday in between. A missing weekday is an expected
//! data absence and makes the comparison unavailable; it never falls back to a
//! filled mark or to another reference.
//!
//! Both series are anchored at 100 on the first invested closing snapshot and
//! the omitted seed-to-first-close interval is disclosed, so the comparison
//! never overwrites NAV's own inception result or its Effective valuation
//! date. Explicit assignments are retrospective: they apply to the whole
//! comparison window, are reported with their assignment date, and are never
//! spliced into an effective-dated chain.
//!
//! The comparison reuses the already ready NAV history and reads the
//! per-asset exposures in one bounded ordered range. It performs no ledger
//! replay, no second NAV rebuild, no per-date query, and no per-date source
//! request: each contributing reference is prepared once through the shared
//! `MarketData` preparation path.
//!
//! Expected missing benchmark inputs are explicit outcome states. A database
//! failure, a corrupt persisted observation, and a violated invariant remain
//! errors, so the Portfolio view never hides a broken ledger or cache behind
//! an unavailable comparison.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, FLOAT_EPSILON};
use crate::db::repos::{
    asset_repo, benchmark_assignment_repo, benchmark_reference_repo, portfolio_asset_history_repo,
};
use crate::models::{
    AssetSnapshot, BenchmarkPeriod, ComparisonAvailability, DatedWealthPoint,
    ExposureBenchmarkComparison, ExposureBenchmarkReference, MarketDataLimitation,
    OmittedSeedInterval, PortfolioSnapshot,
};
use crate::services::benchmark::compute_standard_benchmark_periods;
use crate::services::market_data::MarketData;

/// The subject type of an explicitly assigned Tracked asset.
const TRACKED_SUBJECT_TYPE: &str = "tracked";

/// The expected absence that stops the comparison.
///
/// A corrupt persisted scope, a failed database read, and a violated
/// invariant stay errors; only these two absences are outcome states.
enum IntervalProblem {
    /// The persisted history has no per-asset exposure scope for a date, so
    /// the interval's weights are unknown.
    MissingScope(String),
    /// An exposed asset has no explicit primary reference assignment.
    MissingAssignment(String),
}

/// One interval's exposure weights over the complete NAV scope.
struct IntervalWeights {
    /// The closing snapshot the interval starts at. Its exposures are the
    /// prior-close weights of the interval.
    start_date: String,
    /// The closing snapshot the interval ends at.
    end_date: String,
    /// The aggregated weight of every contributing reference.
    weights: BTreeMap<String, f64>,
}

/// The window in which one reference contributes to the comparison.
///
/// A reference that stops being held is only prepared for the window it was
/// actually held in; a closed position never requires its reference to cover
/// the rest of the comparison.
struct ReferenceWindow {
    start: String,
    end: String,
}

/// The outcome of preparing one contributing reference.
enum ReferencePreparation {
    Prepared(PreparedReference),
    /// The reference cannot contribute; the reason explains why.
    Unavailable(String),
}

/// One prepared reference series.
struct PreparedReference {
    /// The source-reported EUR observations by date. Forward-filled rows are
    /// not observations.
    observations: BTreeMap<String, f64>,
    /// The Market data limitations of the prepared series.
    limitations: Vec<MarketDataLimitation>,
}

/// The result of the comparison calculation: the comparison itself, or the
/// domain reason why it cannot be produced.
enum ComparisonOutcome {
    Available(Box<ExposureBenchmarkComparison>),
    Unavailable(String, Vec<MarketDataLimitation>),
}

/// Computes the exposure-matched portfolio comparison over the ready NAV
/// history.
///
/// The ready snapshots are the Complete NAV snapshots the Portfolio view
/// already obtained; this comparison never replays the ledger, never rebuilds
/// NAV, and never queries one date at a time.
pub async fn compute_exposure_matched_comparison(
    db: &DatabaseConnection,
    market_data: &MarketData,
    ready_snapshots: &[PortfolioSnapshot],
) -> anyhow::Result<ExposureBenchmarkComparison> {
    let Some(anchor_index) = ready_snapshots.iter().position(is_invested_snapshot) else {
        return Ok(ExposureBenchmarkComparison::not_applicable(
            "portfolio has no invested Complete NAV snapshot to anchor an exposure-matched comparison",
        ));
    };
    let anchor = ready_snapshots[anchor_index].clone();
    let effective = ready_snapshots
        .last()
        .context("ready NAV history is not empty")?
        .clone();
    // The comparison follows the market comparison cadence: it ends at the
    // last completed weekday on or before the Effective valuation date, which
    // the Portfolio view keeps unchanged.
    let end = last_weekday_on_or_before(&effective.date)?;
    if end.as_str() < anchor.date.as_str() {
        return Ok(ExposureBenchmarkComparison::not_applicable(
            "portfolio has no completed weekday after its first invested closing snapshot",
        ));
    }

    // One bounded ordered range read replaces a query per date.
    let exposures = portfolio_asset_history_repo::find_between(db, &anchor.date, &end).await?;
    let tickers = load_exposure_tickers(db, &exposures).await?;
    let assignments = load_primary_assignments(db, &tickers).await?;

    // The observed closes: the anchor and every completed weekday after it.
    let observations = observation_dates(&anchor.date, &end)?;
    let (intervals, problem) = interval_weights(
        ready_snapshots,
        &observations,
        &exposures,
        &tickers,
        &assignments,
    )?;

    let reference_ids = contributing_reference_ids(&intervals);
    let definitions = load_reference_definitions(db, &reference_ids).await?;
    let mut provenance = reference_provenance_of_all(&reference_ids, &definitions, &assignments)?;

    if let Some(problem) = problem {
        let limitations = collected_limitations(&provenance);
        let reason = match problem {
            IntervalProblem::MissingScope(date) => format!(
                "the persisted NAV history has no per-asset exposure scope on {date}; the interval weights are unknown"
            ),
            IntervalProblem::MissingAssignment(missing) => format!(
                "held asset {missing} has no explicit primary reference assignment; exposure weights are not renormalized"
            ),
        };
        return Ok(ExposureBenchmarkComparison::unavailable(
            reason,
            provenance.into_values().collect(),
            limitations,
        ));
    }

    let windows = reference_contribution_windows(&intervals);
    let prepared_references =
        match prepare_references(db, market_data, &definitions, &windows, &mut provenance).await? {
            Ok(prepared) => prepared,
            Err(reason) => {
                // A reference cannot contribute. Its identity and its Market data
                // limitations stay visible, and the comparison is unavailable
                // instead of falling back to another reference.
                let limitations = collected_limitations(&provenance);
                return Ok(ExposureBenchmarkComparison::unavailable(
                    reason,
                    provenance.into_values().collect(),
                    limitations,
                ));
            }
        };

    match build_comparison(
        ready_snapshots,
        &anchor,
        &end,
        &intervals,
        &prepared_references,
        &mut provenance,
    )? {
        ComparisonOutcome::Available(comparison) => Ok(*comparison),
        ComparisonOutcome::Unavailable(reason, missing) => {
            let mut limitations = collected_limitations(&provenance);
            limitations.extend(missing);
            Ok(ExposureBenchmarkComparison::unavailable(
                reason,
                provenance.into_values().collect(),
                limitations,
            ))
        }
    }
}

/// The observed closes of the comparison: the anchor and every completed
/// weekday after it.
///
/// A qualified reference observation exists only for completed weekdays, so
/// the anchor must carry one and every weekday in the window must carry one.
fn observation_dates(anchor: &str, end: &str) -> anyhow::Result<Vec<String>> {
    let anchor_date = parse_date(anchor)?;
    let end_date = parse_date(end)?;
    let mut dates = vec![anchor.to_owned()];
    let mut current = anchor_date + Duration::days(1);
    while current <= end_date {
        if is_weekday(current) {
            dates.push(format_date(current));
        }
        current += Duration::days(1);
    }
    Ok(dates)
}

/// The exposure weights of every interval of the comparison.
///
/// Each interval starts at the previous observed close, and that close's
/// per-asset EUR exposures over the complete NAV scope are the prior-close
/// weights of the interval. A same-day trade therefore changes the next
/// interval's weights only.
///
/// The second result is the first exposed asset without an explicit primary
/// assignment. Its weight is not redistributed to the other assets.
fn interval_weights(
    ready_snapshots: &[PortfolioSnapshot],
    observations: &[String],
    exposures: &[AssetSnapshot],
    tickers: &BTreeMap<i32, String>,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignmentRecord>,
) -> anyhow::Result<(Vec<IntervalWeights>, Option<IntervalProblem>)> {
    let mut exposures_by_date: BTreeMap<&str, Vec<&AssetSnapshot>> = BTreeMap::new();
    for exposure in exposures {
        exposures_by_date
            .entry(exposure.date.as_str())
            .or_default()
            .push(exposure);
    }
    let empty: Vec<&AssetSnapshot> = Vec::new();
    let mut missing_assignment: Option<String> = None;

    let mut intervals = Vec::with_capacity(observations.len().saturating_sub(1));
    for window in observations.windows(2) {
        let (start_date, end_date) = (&window[0], &window[1]);
        let snapshot = ready_snapshots
            .iter()
            .find(|snapshot| snapshot.date == *start_date)
            .with_context(|| {
                format!("the ready NAV history has no Complete NAV snapshot on {start_date}")
            })?;
        let rows = exposures_by_date.get(start_date.as_str()).unwrap_or(&empty);
        if let IntervalScope::Unknown = interval_scope(snapshot, rows)? {
            return Ok((
                Vec::new(),
                Some(IntervalProblem::MissingScope(format!(
                    "the persisted NAV history has no per-asset exposure scope on {start_date}"
                ))),
            ));
        }
        let mut weights: BTreeMap<String, f64> = BTreeMap::new();
        for row in rows {
            let ticker = tickers.get(&row.asset_id).with_context(|| {
                format!(
                    "exposure history on {} references unknown asset {}",
                    row.date, row.asset_id
                )
            })?;
            let Some(assignment) = assignments.get(ticker) else {
                missing_assignment.get_or_insert_with(|| ticker.clone());
                continue;
            };
            *weights
                .entry(assignment.reference_id.clone())
                .or_insert(0.0) += row.market_value / snapshot.total_value;
        }
        intervals.push(IntervalWeights {
            start_date: start_date.clone(),
            end_date: end_date.clone(),
            weights,
        });
    }
    let problem = missing_assignment.map(IntervalProblem::MissingAssignment);
    Ok((intervals, problem))
}

/// The complete NAV scope of one closing snapshot.
enum IntervalScope {
    /// The scope is known. The retained Net dividend income is the residual
    /// between the complete NAV scope and the asset value; it is carried at a
    /// 0% return, so it never enters a reference contribution.
    Known,
    /// The persisted history has no per-asset exposure scope for this date, so
    /// the interval's weights are unknown.
    Unknown,
}

/// Reads the complete NAV scope of one closing snapshot.
///
/// The persisted per-asset rows and the persisted asset value describe the
/// same scope. Rows that do not sum to the persisted asset value would double
/// count or drop exposure, so they remain an invariant violation rather than
/// an availability state. A date without any per-asset row has no known
/// exposure scope, which is an expected data absence.
fn interval_scope(
    snapshot: &PortfolioSnapshot,
    exposures: &[&AssetSnapshot],
) -> anyhow::Result<IntervalScope> {
    anyhow::ensure!(
        snapshot.total_value > FLOAT_EPSILON,
        "NAV scope has no value on {}",
        snapshot.date
    );
    let mut asset_value = 0.0;
    for exposure in exposures {
        anyhow::ensure!(
            exposure.market_value.is_finite() && exposure.market_value >= 0.0,
            "invalid persisted exposure for asset {} on {}",
            exposure.asset_id,
            exposure.date
        );
        asset_value += exposure.market_value;
    }

    if exposures.is_empty() {
        if snapshot.asset_value > FLOAT_EPSILON {
            // The persisted history has no per-asset scope for this date.
            return Ok(IntervalScope::Unknown);
        }
    } else {
        let tolerance = snapshot.total_value.abs() * 1e-6;
        anyhow::ensure!(
            (asset_value - snapshot.asset_value).abs() <= tolerance,
            "persisted exposures on {} do not match the persisted asset value",
            snapshot.date
        );
    }

    let retained = snapshot.total_value - snapshot.asset_value;
    let tolerance = snapshot.total_value.abs() * 1e-6;
    anyhow::ensure!(
        retained >= -tolerance,
        "persisted NAV scope on {} is smaller than its asset value",
        snapshot.date
    );
    let _cash_weight = retained.max(0.0) / snapshot.total_value;
    Ok(IntervalScope::Known)
}

/// The window in which each reference contributes to the comparison.
fn reference_contribution_windows(
    intervals: &[IntervalWeights],
) -> BTreeMap<String, ReferenceWindow> {
    let mut windows: BTreeMap<String, ReferenceWindow> = BTreeMap::new();
    for interval in intervals {
        for (reference_id, weight) in &interval.weights {
            if *weight <= 0.0 {
                continue;
            }
            windows
                .entry(reference_id.clone())
                .and_modify(|window| window.end.clone_from(&interval.end_date))
                .or_insert_with(|| ReferenceWindow {
                    start: interval.start_date.clone(),
                    end: interval.end_date.clone(),
                });
        }
    }
    windows
}

/// Prepares every contributing reference through the shared `MarketData`
/// preparation path.
///
/// The outer result carries hard failures; the inner result carries the
/// expected absence that makes the comparison unavailable.
async fn prepare_references(
    db: &DatabaseConnection,
    market_data: &MarketData,
    definitions: &BTreeMap<String, benchmark_reference_repo::BenchmarkReference>,
    windows: &BTreeMap<String, ReferenceWindow>,
    provenance: &mut BTreeMap<String, ExposureBenchmarkReference>,
) -> anyhow::Result<Result<BTreeMap<String, PreparedReference>, String>> {
    let mut prepared_references: BTreeMap<String, PreparedReference> = BTreeMap::new();
    for (reference_id, window) in windows {
        let Some(definition) = definitions.get(reference_id) else {
            anyhow::bail!("assigned benchmark reference '{reference_id}' does not exist");
        };
        match prepare_reference(
            db,
            market_data,
            definition,
            reference_id,
            window,
            provenance,
        )
        .await?
        {
            ReferencePreparation::Prepared(prepared) => {
                prepared_references.insert(reference_id.clone(), prepared);
            }
            ReferencePreparation::Unavailable(reason) => return Ok(Err(reason)),
        }
    }
    Ok(Ok(prepared_references))
}

/// Prepares one contributing reference through the shared `MarketData`
/// preparation path.
///
/// The result is `None` when the reference cannot contribute. The reason and
/// the Market data limitations are recorded on the shared provenance so the
/// outcome keeps them.
async fn prepare_reference(
    db: &DatabaseConnection,
    market_data: &MarketData,
    definition: &benchmark_reference_repo::BenchmarkReference,
    reference_id: &str,
    window: &ReferenceWindow,
    provenance: &mut BTreeMap<String, ExposureBenchmarkReference>,
) -> anyhow::Result<ReferencePreparation> {
    if definition.verification_state != "verified" {
        return Ok(ReferencePreparation::Unavailable(format!(
            "primary reference '{}' has unverified production mapping ({})",
            definition.display_name, definition.verification_state
        )));
    }
    let prepared = market_data
        .selected_benchmark_reference_market_data(db, reference_id, &window.start, &window.end)
        .await?;
    let limitations = prepared.limitations.clone();
    let cadence = prepared
        .source_qualification
        .as_ref()
        .map(|qualification| qualification.cadence.clone());
    if let Some(entry) = provenance.get_mut(reference_id) {
        entry.cadence.clone_from(&cadence);
    }

    let Some(qualification) = prepared.source_qualification.as_ref() else {
        record_limitations(provenance, reference_id, limitations);
        return Ok(ReferencePreparation::Unavailable(
            "selected reference source has no verified return-basis, adjustment, currency, hedge, and cadence qualification".to_owned(),
        ));
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
        || qualification.currency != expected.0
        || qualification.return_basis != expected.1
        || qualification.adjustment != expected.2
        || qualification.hedge_policy != expected.3
    {
        record_limitations(provenance, reference_id, limitations);
        return Ok(ReferencePreparation::Unavailable(
            "selected reference source does not match its explicit definition currency, adjustment, return basis, or hedge policy; required convention is unknown or incompatible".to_owned(),
        ));
    }
    if qualification.cadence != "daily" {
        record_limitations(provenance, reference_id, limitations);
        return Ok(ReferencePreparation::Unavailable(
            "daily exposure weighting requires verified daily reference observations; sparse cadence is not expanded".to_owned(),
        ));
    }
    if !limitations.is_empty() {
        record_limitations(provenance, reference_id, limitations);
        return Ok(ReferencePreparation::Unavailable(
            "required EUR reference coverage is incomplete; see the reference limitations"
                .to_owned(),
        ));
    }

    // Only the source-reported observations are marks. A forward-filled row is
    // not an independent observation and never stands in for a missing one.
    let mut observations = BTreeMap::new();
    for (date, value) in &prepared.genuine_benchmark_series {
        anyhow::ensure!(
            value.is_finite() && *value > 0.0,
            "invalid reference observation for {reference_id} on {date}"
        );
        observations.insert(date.clone(), *value);
    }
    if let Some(missing) = missing_observation(&observations, window) {
        return Ok(ReferencePreparation::Unavailable(format!(
            "primary reference '{}' has no qualified observation on the required close {missing}; forward-filled marks are not independent observations",
            definition.display_name
        )));
    }

    Ok(ReferencePreparation::Prepared(PreparedReference {
        observations,
        limitations,
    }))
}

/// The first required close of a reference's contribution window without a
/// qualified observation.
///
/// The window's endpoints and every completed weekday inside it must carry a
/// source-reported observation.
fn missing_observation(
    observations: &BTreeMap<String, f64>,
    window: &ReferenceWindow,
) -> Option<String> {
    let start = parse_date(&window.start).ok()?;
    let end = parse_date(&window.end).ok()?;
    let mut current = start;
    while current <= end {
        let date = format_date(current);
        if (is_weekday(current) || date == window.start || date == window.end)
            && !observations.contains_key(&date)
        {
            return Some(date);
        }
        current += Duration::days(1);
    }
    None
}

/// Builds the aligned subject/reference comparison over the ready NAV
/// history.
fn build_comparison(
    ready_snapshots: &[PortfolioSnapshot],
    anchor: &PortfolioSnapshot,
    end: &str,
    intervals: &[IntervalWeights],
    prepared_references: &BTreeMap<String, PreparedReference>,
    provenance: &mut BTreeMap<String, ExposureBenchmarkReference>,
) -> anyhow::Result<ComparisonOutcome> {
    let anchor_nav = anchor.nav;
    let mut subject_wealth = vec![DatedWealthPoint {
        date: anchor.date.clone(),
        wealth: 100.0,
    }];
    let mut reference_wealth = vec![DatedWealthPoint {
        date: anchor.date.clone(),
        wealth: 100.0,
    }];
    let mut reference_weights: BTreeMap<String, ReferenceWeight> = BTreeMap::new();
    let mut reference_value: f64 = 100.0;

    for interval in intervals {
        let interval_return = match interval_reference_return(
            interval,
            prepared_references,
            &mut reference_weights,
        ) {
            Ok(interval_return) => interval_return,
            Err(reason) => return Ok(ComparisonOutcome::Unavailable(reason, Vec::new())),
        };
        let current_nav = ready_snapshots
            .iter()
            .find(|snapshot| snapshot.date == interval.end_date)
            .map(|snapshot| snapshot.nav)
            .with_context(|| {
                format!(
                    "the ready NAV history has no Complete NAV snapshot on {}",
                    interval.end_date
                )
            })?;
        reference_value *= 1.0 + interval_return;
        let subject_value = 100.0 * current_nav / anchor_nav;
        anyhow::ensure!(
            subject_value.is_finite() && subject_value > 0.0 && reference_value.is_finite(),
            "unusable comparison value on {}",
            interval.end_date
        );
        subject_wealth.push(DatedWealthPoint {
            date: interval.end_date.clone(),
            wealth: subject_value,
        });
        reference_wealth.push(DatedWealthPoint {
            date: interval.end_date.clone(),
            wealth: reference_value,
        });
    }

    let subject_series: Vec<(String, f64)> = subject_wealth
        .iter()
        .map(|point| (point.date.clone(), point.wealth))
        .collect();
    let reference_series: Vec<(String, f64)> = reference_wealth
        .iter()
        .map(|point| (point.date.clone(), point.wealth))
        .collect();

    // The standard rows are measured on the completed-weekday observations of
    // the comparison window. The graph and the cumulative returns above cover
    // every observed close, and every row exposes its own actual anchor.
    let standard_periods =
        compute_standard_benchmark_periods(&subject_series, &reference_series, &anchor.date, end)?;

    for (reference_id, weights) in &reference_weights {
        if let Some(entry) = provenance.get_mut(reference_id) {
            entry.first_weight_pct = Some(weights.first_weight * 100.0);
            entry.last_weight_pct = Some(weights.last_weight * 100.0);
            entry.max_weight_pct = Some(weights.max_weight * 100.0);
        }
    }

    let reference_limitations: Vec<MarketDataLimitation> = prepared_references
        .values()
        .flat_map(|reference| reference.limitations.iter().cloned())
        .collect();

    let subject_return =
        (subject_series[subject_series.len() - 1].1 / subject_series[0].1 - 1.0) * 100.0;
    let reference_return =
        (reference_series[reference_series.len() - 1].1 / reference_series[0].1 - 1.0) * 100.0;

    Ok(ComparisonOutcome::Available(Box::new(comparison_outcome(
        ready_snapshots,
        anchor,
        end,
        &subject_wealth,
        &reference_wealth,
        standard_periods,
        provenance,
        reference_limitations,
        subject_return,
        reference_return,
    ))))
}

/// Assembles the available comparison outcome.
#[allow(clippy::too_many_arguments)]
fn comparison_outcome(
    ready_snapshots: &[PortfolioSnapshot],
    anchor: &PortfolioSnapshot,
    end: &str,
    subject_wealth: &[DatedWealthPoint],
    reference_wealth: &[DatedWealthPoint],
    standard_periods: Vec<BenchmarkPeriod>,
    provenance: &BTreeMap<String, ExposureBenchmarkReference>,
    reference_limitations: Vec<MarketDataLimitation>,
    subject_return: f64,
    reference_return: f64,
) -> ExposureBenchmarkComparison {
    ExposureBenchmarkComparison {
        availability: ComparisonAvailability::Available,
        role: "primary".to_owned(),
        method: "exposure_matched".to_owned(),
        subject_label: "Portfolio".to_owned(),
        reference_label: "Exposure-matched benchmark".to_owned(),
        currency: crate::constants::BASE_CURRENCY.to_owned(),
        start_date: Some(anchor.date.clone()),
        end_date: Some(end.to_owned()),
        inception_date: ready_snapshots
            .first()
            .map(|snapshot| snapshot.date.clone()),
        omitted_seed_interval: ready_snapshots
            .iter()
            .position(|snapshot| snapshot.date == anchor.date)
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| ready_snapshots.get(index))
            .map(|seed| OmittedSeedInterval {
                seed_date: seed.date.clone(),
                anchor_date: anchor.date.clone(),
            }),
        retrospective: true,
        subject_return_pct: Some(subject_return),
        reference_return_pct: Some(reference_return),
        excess_return_percentage_points: Some(subject_return - reference_return),
        subject_wealth: subject_wealth.to_vec(),
        reference_wealth: reference_wealth.to_vec(),
        standard_periods,
        references: provenance.values().cloned().collect(),
        reference_limitations,
    }
}

/// The exposure-weighted reference return of one interval.
///
/// Every contributing reference is observed in every interval, so a reference
/// whose asset was sold reports a zero weight afterwards instead of keeping
/// its last held weight.
fn interval_reference_return(
    interval: &IntervalWeights,
    prepared_references: &BTreeMap<String, PreparedReference>,
    reference_weights: &mut BTreeMap<String, ReferenceWeight>,
) -> Result<f64, String> {
    let mut weighted_return = 0.0;
    for (reference_id, weight) in prepared_references
        .keys()
        .map(|reference_id| {
            (
                reference_id.clone(),
                interval.weights.get(reference_id).copied().unwrap_or(0.0),
            )
        })
        .collect::<Vec<(String, f64)>>()
    {
        if weight <= 0.0 {
            // A reference without weight contributes no return; its own series
            // is not required for this interval.
            reference_weights
                .entry(reference_id)
                .or_default()
                .observe(0.0);
            continue;
        }
        let Some(reference) = prepared_references.get(&reference_id) else {
            return Err(format!(
                "reference {reference_id} was not prepared for the comparison window"
            ));
        };
        let Some(start_value) = reference.observations.get(&interval.start_date) else {
            return Err(format!(
                "reference {reference_id} has no qualified observation on the closing snapshot {}",
                interval.start_date
            ));
        };
        let Some(end_value) = reference.observations.get(&interval.end_date) else {
            return Err(format!(
                "reference {reference_id} has no qualified observation on the closing snapshot {}",
                interval.end_date
            ));
        };
        weighted_return += weight * (end_value / start_value - 1.0);
        reference_weights
            .entry(reference_id)
            .or_default()
            .observe(weight);
    }
    Ok(weighted_return)
}

/// One contributing reference with the aggregate weight it carried.
#[derive(Clone, Debug, Default)]
struct ReferenceWeight {
    first_weight: f64,
    last_weight: f64,
    max_weight: f64,
    observations: usize,
}

impl ReferenceWeight {
    fn observe(&mut self, weight: f64) {
        if self.observations == 0 {
            self.first_weight = weight;
        }
        self.last_weight = weight;
        self.max_weight = self.max_weight.max(weight);
        self.observations += 1;
    }
}

/// The provenance of every contributing reference definition.
fn reference_provenance_of_all(
    reference_ids: &[String],
    definitions: &BTreeMap<String, benchmark_reference_repo::BenchmarkReference>,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignmentRecord>,
) -> anyhow::Result<BTreeMap<String, ExposureBenchmarkReference>> {
    let mut provenance: BTreeMap<String, ExposureBenchmarkReference> = BTreeMap::new();
    for reference_id in reference_ids {
        let Some(definition) = definitions.get(reference_id) else {
            // The assignment points at a definition that does not exist. The
            // foreign key prevents it, so it is a corrupt catalog.
            anyhow::bail!("assigned benchmark reference '{reference_id}' does not exist");
        };
        provenance.insert(
            reference_id.clone(),
            reference_provenance_of(
                definition,
                &assigned_assets(reference_id, assignments),
                assigned_at(reference_id, assignments),
                None,
            ),
        );
    }
    Ok(provenance)
}

/// The distinct primary references the weight-bearing assets contribute to.
fn contributing_reference_ids(intervals: &[IntervalWeights]) -> Vec<String> {
    let mut reference_ids: BTreeSet<String> = BTreeSet::new();
    for interval in intervals {
        for (reference_id, weight) in &interval.weights {
            if *weight > 0.0 {
                reference_ids.insert(reference_id.clone());
            }
        }
    }
    reference_ids.into_iter().collect()
}

/// Loads the definition of every contributing reference.
async fn load_reference_definitions(
    db: &DatabaseConnection,
    reference_ids: &[String],
) -> anyhow::Result<BTreeMap<String, benchmark_reference_repo::BenchmarkReference>> {
    let mut definitions = BTreeMap::new();
    for reference_id in reference_ids {
        let definition = benchmark_reference_repo::find_by_id(db, reference_id).await?;
        definitions.insert(reference_id.clone(), definition);
    }
    Ok(definitions)
}

/// The provenance of one selected reference definition.
fn reference_provenance_of(
    definition: &benchmark_reference_repo::BenchmarkReference,
    assets: &[String],
    assigned_at: Option<String>,
    cadence: Option<String>,
) -> ExposureBenchmarkReference {
    ExposureBenchmarkReference {
        reference_id: definition.id.clone(),
        display_name: definition.display_name.clone(),
        assets: assets.to_vec(),
        first_weight_pct: None,
        last_weight_pct: None,
        max_weight_pct: None,
        native_currency: definition.currency.clone(),
        comparison_currency: crate::constants::BASE_CURRENCY.to_owned(),
        return_basis: definition.return_basis_qualification.clone(),
        adjustment: definition.adjustment_qualification.clone(),
        hedge_policy: definition.hedge_policy.clone(),
        cadence,
        source: definition.source.clone(),
        source_identifier: definition.source_identifier.clone(),
        source_provenance: definition.source_provenance.clone(),
        verification_state: definition.verification_state.clone(),
        assigned_at,
        limitations: Vec::new(),
    }
}

/// The tracked assets assigned to one primary reference.
fn assigned_assets(
    reference_id: &str,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignmentRecord>,
) -> Vec<String> {
    let mut assets: Vec<String> = assignments
        .iter()
        .filter(|(_, assignment)| assignment.reference_id == reference_id)
        .map(|(ticker, _)| ticker.clone())
        .collect();
    assets.sort();
    assets
}

/// The most recent explicit assignment of one primary reference.
fn assigned_at(
    reference_id: &str,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignmentRecord>,
) -> Option<String> {
    assignments
        .values()
        .filter(|assignment| assignment.reference_id == reference_id)
        .map(|assignment| assignment.assigned_at.clone())
        .max()
}

/// The Market data limitations of every prepared or unmeasurable reference.
fn collected_limitations(
    provenance: &BTreeMap<String, ExposureBenchmarkReference>,
) -> Vec<MarketDataLimitation> {
    provenance
        .values()
        .flat_map(|entry| entry.limitations.iter().cloned())
        .collect()
}

/// Records the Market data limitations of a reference that could not
/// contribute.
fn record_limitations(
    provenance: &mut BTreeMap<String, ExposureBenchmarkReference>,
    reference_id: &str,
    limitations: Vec<MarketDataLimitation>,
) {
    if let Some(entry) = provenance.get_mut(reference_id) {
        entry.limitations = limitations;
    }
}

/// Loads the ticker of every asset that appears in the exposure history.
async fn load_exposure_tickers(
    db: &DatabaseConnection,
    exposures: &[AssetSnapshot],
) -> anyhow::Result<BTreeMap<i32, String>> {
    let asset_ids: BTreeSet<i32> = exposures.iter().map(|exposure| exposure.asset_id).collect();
    let assets = asset_repo::find_by_ids(db, asset_ids.iter().copied()).await?;
    let mut tickers = BTreeMap::new();
    for asset in assets {
        if asset.is_monetary() {
            // Monetary holdings are excluded from performance measurement and
            // never carry an exposure weight.
            continue;
        }
        tickers.insert(asset.id, asset.ticker);
    }
    Ok(tickers)
}

/// Loads the explicit primary assignments of the exposed assets.
async fn load_primary_assignments(
    db: &DatabaseConnection,
    tickers: &BTreeMap<i32, String>,
) -> anyhow::Result<BTreeMap<String, benchmark_assignment_repo::PrimaryAssignmentRecord>> {
    let subjects: Vec<String> = tickers.values().cloned().collect();
    let assignments =
        benchmark_assignment_repo::find_by_subjects(db, TRACKED_SUBJECT_TYPE, &subjects).await?;
    Ok(assignments
        .into_iter()
        .map(|assignment| (assignment.subject_id.clone(), assignment))
        .collect())
}

/// A Complete NAV snapshot carries a value once the portfolio is invested.
fn is_invested_snapshot(snapshot: &PortfolioSnapshot) -> bool {
    snapshot.total_value > FLOAT_EPSILON
}

/// The last completed weekday on or before the given date.
fn last_weekday_on_or_before(date: &str) -> anyhow::Result<String> {
    let parsed = parse_date(date)?;
    let mut candidate = parsed;
    while !is_weekday(candidate) {
        candidate -= Duration::days(1);
    }
    Ok(format_date(candidate))
}

/// Parses one stored date.
fn parse_date(date: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, crate::constants::DATE_FORMAT)
        .with_context(|| format!("invalid comparison date {date}"))
}

/// A completed weekday is Monday through Friday.
fn is_weekday(date: NaiveDate) -> bool {
    matches!(
        date.weekday(),
        Weekday::Mon | Weekday::Tue | Weekday::Wed | Weekday::Thu | Weekday::Fri
    )
}
