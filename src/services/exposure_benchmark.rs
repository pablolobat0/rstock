//! Exposure-matched portfolio benchmark.
//!
//! The comparison is the primary benchmark of the Portfolio dashboard. It
//! combines the explicitly assigned primary reference of every held asset
//! through the EUR exposures of the *preceding* Complete NAV snapshot, so
//! the weights that measured a past interval are the exposures that were
//! actually held at the previous close. Today's holdings are never projected
//! backwards, and a same-day trade only changes the next day's weights.
//!
//! The complete NAV scope is the denominator of every weight. Monetary
//! holdings are excluded, and the retained Net dividend income inside NAV is
//! carried as a 0%-return residual. Several assets assigned the same primary
//! reference aggregate their weights into one contribution instead of being
//! counted twice, and secondary references never enter the weights. A missing
//! assignment or a missing required component makes the comparison
//! unavailable instead of renormalizing the remaining weights.
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

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, FLOAT_EPSILON};
use crate::db::repos::{
    asset_repo, benchmark_assignment_repo, benchmark_reference_repo, portfolio_asset_history_repo,
};
use crate::models::{
    AssetSnapshot, ComparisonAvailability, DatedWealthPoint, ExposureBenchmarkComparison,
    ExposureBenchmarkReference, MarketDataLimitation, OmittedSeedInterval, PortfolioSnapshot,
};
use crate::services::benchmark::compute_standard_benchmark_periods;
use crate::services::market_data::MarketData;

/// The subject type of an explicitly assigned Tracked asset.
const TRACKED_SUBJECT_TYPE: &str = "tracked";

/// The weight of one held asset inside the complete NAV scope of one closing
/// snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct AssetExposureWeight {
    pub asset_id: i32,
    pub ticker: String,
    /// The asset's EUR market value at that close.
    pub market_value: f64,
    /// The asset's share of the complete NAV scope.
    pub weight: f64,
}

/// One closing snapshot's exposure weights.
///
/// The weights come from the snapshot's own per-asset EUR market values over
/// the complete NAV scope, and the retained Net dividend income is the
/// residual carried at a 0% return.
#[derive(Clone, Debug, PartialEq)]
pub struct IntervalExposure {
    /// The closing snapshot the weights belong to.
    pub date: String,
    pub asset_weights: Vec<AssetExposureWeight>,
    /// The retained Net dividend income residual at a 0% return.
    pub cash_weight: f64,
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

/// One prepared reference series with its definition provenance.
struct PreparedReference {
    definition: benchmark_reference_repo::BenchmarkReference,
    /// The EUR-converted reference values by date, using the same
    /// forward-filled convention as the NAV series they are aligned with.
    values: BTreeMap<String, f64>,
    cadence: Option<String>,
    limitations: Vec<MarketDataLimitation>,
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

/// The result of the comparison calculation: the comparison itself, or the
/// domain reason why it cannot be produced.
enum ComparisonOutcome {
    Available(Box<ExposureBenchmarkComparison>),
    Unavailable(String),
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

    let weight_bearing = weight_bearing_assets(&exposures, &end);
    if let Some(missing) = missing_assignment(&weight_bearing, &tickers, &assignments) {
        return Ok(ExposureBenchmarkComparison::unavailable(format!(
            "held asset {missing} has no explicit primary reference assignment; exposure weights are not renormalized"
        )));
    }

    let mut prepared_references: BTreeMap<String, PreparedReference> = BTreeMap::new();
    let reference_ids = contributing_reference_ids(&weight_bearing, &tickers, &assignments);
    let windows =
        reference_contribution_windows(&exposures, &end, &tickers, &assignments, ready_snapshots)?;
    for reference_id in &reference_ids {
        let Some(window) = windows.get(reference_id) else {
            return Ok(ExposureBenchmarkComparison::unavailable(format!(
                "reference {reference_id} has no exposure window in the comparison"
            )));
        };
        match prepare_reference(db, market_data, reference_id, &window.start, &window.end).await? {
            Ok(prepared) => {
                prepared_references.insert(reference_id.clone(), prepared);
            }
            Err(reason) => return Ok(ExposureBenchmarkComparison::unavailable(reason)),
        }
    }

    match build_comparison(
        ready_snapshots,
        anchor_index,
        &end,
        &exposures,
        &tickers,
        &assignments,
        &prepared_references,
        &reference_ids,
    )? {
        ComparisonOutcome::Available(comparison) => Ok(*comparison),
        ComparisonOutcome::Unavailable(reason) => {
            Ok(ExposureBenchmarkComparison::unavailable(reason))
        }
    }
}

/// Computes one closing snapshot's exposure weights.
///
/// The per-asset EUR market values of the snapshot are the weights' numerators
/// and the complete NAV scope is the denominator. The retained Net dividend
/// income is the residual between the complete NAV scope and the asset value.
pub fn compute_interval_exposure(
    snapshot: &PortfolioSnapshot,
    exposures: &[&AssetSnapshot],
    tickers: &BTreeMap<i32, String>,
) -> anyhow::Result<IntervalExposure> {
    let (_, cash_weight) = interval_scope(snapshot, exposures)?;
    let mut asset_weights = Vec::with_capacity(exposures.len());
    for exposure in exposures {
        let ticker = tickers.get(&exposure.asset_id).with_context(|| {
            format!(
                "exposure history on {} references unknown asset {}",
                exposure.date, exposure.asset_id
            )
        })?;
        asset_weights.push(AssetExposureWeight {
            asset_id: exposure.asset_id,
            ticker: ticker.clone(),
            market_value: exposure.market_value,
            weight: exposure.market_value / snapshot.total_value,
        });
    }
    asset_weights.sort_by(|left, right| left.ticker.cmp(&right.ticker));

    Ok(IntervalExposure {
        date: snapshot.date.clone(),
        asset_weights,
        cash_weight,
    })
}

/// The complete NAV scope of one closing snapshot: the persisted asset value
/// and the retained Net dividend income residual.
///
/// The persisted per-asset rows and the persisted asset value describe the
/// same scope. A mismatch would double count or drop exposure.
fn interval_scope(
    snapshot: &PortfolioSnapshot,
    exposures: &[&AssetSnapshot],
) -> anyhow::Result<(f64, f64)> {
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

    let tolerance = snapshot.total_value.abs() * 1e-6;
    anyhow::ensure!(
        (asset_value - snapshot.asset_value).abs() <= tolerance,
        "persisted exposures on {} do not match the persisted asset value",
        snapshot.date
    );

    let retained = snapshot.total_value - snapshot.asset_value;
    anyhow::ensure!(
        retained >= -tolerance,
        "persisted NAV scope on {} is smaller than its asset value",
        snapshot.date
    );
    Ok((asset_value, retained.max(0.0) / snapshot.total_value))
}

/// Builds the aligned subject/reference comparison over the ready NAV
/// history.
#[allow(clippy::too_many_arguments)]
fn build_comparison(
    ready_snapshots: &[PortfolioSnapshot],
    anchor_index: usize,
    end: &str,
    exposures: &[AssetSnapshot],
    tickers: &BTreeMap<i32, String>,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignment>,
    prepared_references: &BTreeMap<String, PreparedReference>,
    reference_ids: &[String],
) -> anyhow::Result<ComparisonOutcome> {
    let anchor = &ready_snapshots[anchor_index];
    let end_index = ready_snapshots
        .iter()
        .rposition(|snapshot| snapshot.date.as_str() == end)
        .context("the comparison end is part of the ready NAV history")?;

    let series = match build_series(
        ready_snapshots,
        anchor_index,
        end_index,
        exposures,
        tickers,
        assignments,
        prepared_references,
        reference_ids,
    )? {
        Ok(series) => series,
        Err(reason) => return Ok(ComparisonOutcome::Unavailable(reason)),
    };
    let subject_wealth = series.subject_wealth;
    let reference_wealth = series.reference_wealth;
    let reference_weights = series.reference_weights;

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
    // every closing snapshot, and every row exposes its own actual anchor.
    let period_start = first_weekday_on_or_after(&anchor.date)?;
    let standard_periods =
        compute_standard_benchmark_periods(&subject_series, &reference_series, &period_start, end)?;

    let references = reference_provenance(&reference_weights, prepared_references, assignments);

    let reference_limitations = prepared_references
        .values()
        .flat_map(|reference| reference.limitations.iter().cloned())
        .collect();

    let subject_return =
        (subject_series[subject_series.len() - 1].1 / subject_series[0].1 - 1.0) * 100.0;
    let reference_return =
        (reference_series[reference_series.len() - 1].1 / reference_series[0].1 - 1.0) * 100.0;

    Ok(ComparisonOutcome::Available(Box::new(
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
            omitted_seed_interval: anchor_index
                .checked_sub(1)
                .and_then(|index| ready_snapshots.get(index))
                .map(|seed| OmittedSeedInterval {
                    seed_date: seed.date.clone(),
                    anchor_date: anchor.date.clone(),
                }),
            retrospective: true,
            subject_return_pct: Some(subject_return),
            reference_return_pct: Some(reference_return),
            excess_return_percentage_points: Some(subject_return - reference_return),
            subject_wealth,
            reference_wealth,
            standard_periods,
            references,
            reference_limitations,
        },
    )))
}

/// The aligned series of one comparison window.
struct ComparisonSeries {
    subject_wealth: Vec<DatedWealthPoint>,
    reference_wealth: Vec<DatedWealthPoint>,
    reference_weights: BTreeMap<String, ReferenceWeight>,
}

/// Builds the aligned subject and reference series.
///
/// Every interval uses the exposures of the preceding closing snapshot, and
/// the reference return of each interval is the weighted sum of the assigned
/// primary reference returns over that snapshot's weights.
#[allow(clippy::too_many_arguments)]
fn build_series(
    ready_snapshots: &[PortfolioSnapshot],
    anchor_index: usize,
    end_index: usize,
    exposures: &[AssetSnapshot],
    tickers: &BTreeMap<i32, String>,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignment>,
    prepared_references: &BTreeMap<String, PreparedReference>,
    reference_ids: &[String],
) -> anyhow::Result<Result<ComparisonSeries, String>> {
    let anchor = &ready_snapshots[anchor_index];
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
    // The persisted exposures are read in ascending chronology, so one forward
    // cursor serves every interval instead of a per-date lookup.
    let mut cursor = 0usize;

    for index in (anchor_index + 1)..=end_index {
        let previous = &ready_snapshots[index - 1];
        let current = &ready_snapshots[index];
        while cursor < exposures.len() && exposures[cursor].date.as_str() < previous.date.as_str() {
            cursor += 1;
        }
        let start = cursor;
        while cursor < exposures.len() && exposures[cursor].date.as_str() == previous.date.as_str()
        {
            cursor += 1;
        }
        let rows: Vec<&AssetSnapshot> = exposures[start..cursor].iter().collect();
        let (_, _cash_weight) = interval_scope(previous, &rows)?;
        let weights_by_reference =
            interval_reference_weights(&rows, previous, tickers, assignments, reference_ids)?;

        let mut weighted_return = 0.0;
        for (reference_id, weight) in &weights_by_reference {
            if *weight <= 0.0 {
                // A reference without weight contributes no return; its own
                // series is not required for this interval.
                reference_weights
                    .entry((*reference_id).to_owned())
                    .or_default()
                    .observe(0.0);
                continue;
            }
            let Some(reference) = prepared_references.get(*reference_id) else {
                return Ok(Err(format!(
                    "reference {reference_id} was not prepared for the comparison window"
                )));
            };
            let Some(start_value) = reference.values.get(&previous.date) else {
                return Ok(Err(format!(
                    "reference {reference_id} has no observation on the closing snapshot {}",
                    previous.date
                )));
            };
            let Some(end_value) = reference.values.get(&current.date) else {
                return Ok(Err(format!(
                    "reference {reference_id} has no observation on the closing snapshot {}",
                    current.date
                )));
            };
            if !start_value.is_finite()
                || *start_value <= 0.0
                || !end_value.is_finite()
                || *end_value <= 0.0
            {
                return Ok(Err(format!(
                    "reference {reference_id} has an unusable observation on {}",
                    current.date
                )));
            }
            weighted_return += weight * (end_value / start_value - 1.0);
            reference_weights
                .entry((*reference_id).to_owned())
                .or_default()
                .observe(*weight);
        }

        reference_value *= 1.0 + weighted_return;
        let subject_value = 100.0 * current.nav / anchor.nav;
        anyhow::ensure!(
            subject_value.is_finite() && subject_value > 0.0 && reference_value.is_finite(),
            "unusable comparison value on {}",
            current.date
        );
        subject_wealth.push(DatedWealthPoint {
            date: current.date.clone(),
            wealth: subject_value,
        });
        reference_wealth.push(DatedWealthPoint {
            date: current.date.clone(),
            wealth: reference_value,
        });
    }

    Ok(Ok(ComparisonSeries {
        subject_wealth,
        reference_wealth,
        reference_weights,
    }))
}

/// The aggregated weight of every contributing reference in one interval.
///
/// Every contributing reference is observed in every interval, so a reference
/// whose asset was sold reports a zero weight afterwards instead of keeping
/// its last held weight.
fn interval_reference_weights<'a>(
    rows: &[&AssetSnapshot],
    snapshot: &PortfolioSnapshot,
    tickers: &BTreeMap<i32, String>,
    assignments: &'a BTreeMap<String, benchmark_assignment_repo::PrimaryAssignment>,
    reference_ids: &'a [String],
) -> anyhow::Result<BTreeMap<&'a str, f64>> {
    let mut weights_by_reference: BTreeMap<&'a str, f64> = BTreeMap::new();
    for reference_id in reference_ids {
        weights_by_reference.insert(reference_id.as_str(), 0.0);
    }
    for row in rows {
        let ticker = tickers.get(&row.asset_id).with_context(|| {
            format!(
                "exposure history on {} references unknown asset {}",
                row.date, row.asset_id
            )
        })?;
        let assignment = assignments
            .get(ticker)
            .with_context(|| format!("missing primary assignment for held asset {ticker}"))?;
        *weights_by_reference
            .entry(assignment.reference_id.as_str())
            .or_insert(0.0) += row.market_value / snapshot.total_value;
    }
    Ok(weights_by_reference)
}

/// Collects the provenance of every contributing reference.
fn reference_provenance(
    reference_weights: &BTreeMap<String, ReferenceWeight>,
    prepared_references: &BTreeMap<String, PreparedReference>,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignment>,
) -> Vec<ExposureBenchmarkReference> {
    let mut assets_by_reference: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut assigned_at: BTreeMap<&str, &str> = BTreeMap::new();
    for (ticker, assignment) in assignments {
        assets_by_reference
            .entry(assignment.reference_id.as_str())
            .or_default()
            .insert(ticker.as_str());
        assigned_at
            .entry(assignment.reference_id.as_str())
            .and_modify(|current| {
                if assignment.assigned_at.as_str() > *current {
                    *current = assignment.assigned_at.as_str();
                }
            })
            .or_insert_with(|| assignment.assigned_at.as_str());
    }

    reference_weights
        .iter()
        .filter_map(|(reference_id, weights)| {
            let prepared = prepared_references.get(reference_id)?;
            Some(ExposureBenchmarkReference {
                reference_id: prepared.definition.id.clone(),
                display_name: prepared.definition.display_name.clone(),
                assets: assets_by_reference
                    .get(reference_id.as_str())
                    .map(|assets| assets.iter().map(|asset| (*asset).to_owned()).collect())
                    .unwrap_or_default(),
                first_weight_pct: weights.first_weight * 100.0,
                last_weight_pct: weights.last_weight * 100.0,
                max_weight_pct: weights.max_weight * 100.0,
                native_currency: prepared.definition.currency.clone(),
                comparison_currency: crate::constants::BASE_CURRENCY.to_owned(),
                return_basis: prepared.definition.return_basis_qualification.clone(),
                adjustment: prepared.definition.adjustment_qualification.clone(),
                hedge_policy: prepared.definition.hedge_policy.clone(),
                cadence: prepared.cadence.clone(),
                source: prepared.definition.source.clone(),
                source_identifier: prepared.definition.source_identifier.clone(),
                source_provenance: prepared.definition.source_provenance.clone(),
                verification_state: prepared.definition.verification_state.clone(),
                assigned_at: assigned_at
                    .get(reference_id.as_str())
                    .map(|date| (*date).to_owned()),
            })
        })
        .collect()
}

/// Prepares one contributing reference through the shared `MarketData`
/// preparation path.
///
/// The outer result carries hard failures; the inner result carries the
/// domain absences that make the comparison unavailable.
async fn prepare_reference(
    db: &DatabaseConnection,
    market_data: &MarketData,
    reference_id: &str,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Result<PreparedReference, String>> {
    let definition = benchmark_reference_repo::find_by_id(db, reference_id).await?;
    if definition.verification_state != "verified" {
        return Ok(Err(format!(
            "primary reference '{}' has unverified production mapping ({})",
            definition.display_name, definition.verification_state
        )));
    }
    let prepared = market_data
        .selected_benchmark_reference_market_data(db, reference_id, start_date, end_date)
        .await?;
    if !prepared.limitations.is_empty() {
        return Ok(Err(
            "required EUR reference coverage is incomplete; see the reference limitations"
                .to_owned(),
        ));
    }
    let Some(qualification) = prepared.source_qualification.as_ref() else {
        return Ok(Err(
            "selected reference source has no verified return-basis, adjustment, currency, hedge, and cadence qualification"
                .to_owned(),
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
        return Ok(Err(
            "selected reference source does not match its explicit definition currency, adjustment, return basis, or hedge policy; required convention is unknown or incompatible"
                .to_owned(),
        ));
    }
    if qualification.cadence != "daily" {
        return Ok(Err(
            "daily exposure weighting requires verified daily reference observations; sparse cadence is not expanded"
                .to_owned(),
        ));
    }

    let mut values = BTreeMap::new();
    for (date, value) in &prepared.benchmark_series {
        anyhow::ensure!(
            value.is_finite() && *value > 0.0,
            "invalid reference observation for {reference_id} on {date}"
        );
        values.insert(date.clone(), *value);
    }
    Ok(Ok(PreparedReference {
        definition,
        values,
        cadence: Some(qualification.cadence.clone()),
        limitations: prepared.limitations,
    }))
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
) -> anyhow::Result<BTreeMap<String, benchmark_assignment_repo::PrimaryAssignment>> {
    let subjects: Vec<String> = tickers.values().cloned().collect();
    let assignments =
        benchmark_assignment_repo::find_by_subjects(db, TRACKED_SUBJECT_TYPE, &subjects).await?;
    Ok(assignments
        .into_iter()
        .map(|assignment| (assignment.subject_id.clone(), assignment))
        .collect())
}

/// The assets whose exposures carry weight in at least one interval: every
/// asset exposed before the last closing snapshot of the window.
fn weight_bearing_assets(exposures: &[AssetSnapshot], end_date: &str) -> BTreeSet<i32> {
    exposures
        .iter()
        .filter(|exposure| exposure.date.as_str() < end_date)
        .map(|exposure| exposure.asset_id)
        .collect()
}

/// The first exposed asset without an explicit primary assignment.
fn missing_assignment(
    weight_bearing: &BTreeSet<i32>,
    tickers: &BTreeMap<i32, String>,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignment>,
) -> Option<String> {
    weight_bearing
        .iter()
        .filter_map(|asset_id| tickers.get(asset_id))
        .find(|ticker| !assignments.contains_key(*ticker))
        .cloned()
}

/// The distinct primary references the weight-bearing assets contribute to.
fn contributing_reference_ids(
    weight_bearing: &BTreeSet<i32>,
    tickers: &BTreeMap<i32, String>,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignment>,
) -> Vec<String> {
    let mut reference_ids: BTreeSet<String> = BTreeSet::new();
    for asset_id in weight_bearing {
        let Some(ticker) = tickers.get(asset_id) else {
            continue;
        };
        if let Some(assignment) = assignments.get(ticker) {
            reference_ids.insert(assignment.reference_id.clone());
        }
    }
    reference_ids.into_iter().collect()
}

/// The window in which each reference actually contributes to the comparison.
///
/// A reference is only prepared for the closing snapshots it was held in, so a
/// closed position never requires its reference to cover the rest of the
/// comparison window.
fn reference_contribution_windows(
    exposures: &[AssetSnapshot],
    comparison_end: &str,
    tickers: &BTreeMap<i32, String>,
    assignments: &BTreeMap<String, benchmark_assignment_repo::PrimaryAssignment>,
    ready_snapshots: &[PortfolioSnapshot],
) -> anyhow::Result<BTreeMap<String, ReferenceWindow>> {
    let mut first: BTreeMap<String, String> = BTreeMap::new();
    let mut last: BTreeMap<String, String> = BTreeMap::new();
    for exposure in exposures
        .iter()
        .filter(|exposure| exposure.date.as_str() < comparison_end)
        .filter(|exposure| exposure.market_value > 0.0)
    {
        let Some(ticker) = tickers.get(&exposure.asset_id) else {
            continue;
        };
        let Some(assignment) = assignments.get(ticker) else {
            continue;
        };
        first
            .entry(assignment.reference_id.clone())
            .or_insert_with(|| exposure.date.clone());
        last.insert(assignment.reference_id.clone(), exposure.date.clone());
    }

    let mut windows = BTreeMap::new();
    for (reference_id, start) in first {
        let last_date = last
            .get(&reference_id)
            .cloned()
            .unwrap_or_else(|| start.clone());
        let end = next_snapshot_date(ready_snapshots, &last_date)?;
        windows.insert(reference_id, ReferenceWindow { start, end });
    }
    Ok(windows)
}

/// The closing snapshot that follows the given date.
fn next_snapshot_date(ready_snapshots: &[PortfolioSnapshot], date: &str) -> anyhow::Result<String> {
    ready_snapshots
        .iter()
        .find(|snapshot| snapshot.date.as_str() > date)
        .map(|snapshot| snapshot.date.clone())
        .with_context(|| format!("no closing snapshot follows {date}"))
}

/// A Complete NAV snapshot carries a value once the portfolio is invested.
fn is_invested_snapshot(snapshot: &PortfolioSnapshot) -> bool {
    snapshot.total_value > FLOAT_EPSILON
}

/// The first completed weekday on or after the given date.
fn first_weekday_on_or_after(date: &str) -> anyhow::Result<String> {
    let parsed = NaiveDate::parse_from_str(date, crate::constants::DATE_FORMAT)
        .with_context(|| format!("invalid comparison date {date}"))?;
    let mut candidate = parsed;
    while matches!(candidate.weekday(), Weekday::Sat | Weekday::Sun) {
        candidate += Duration::days(1);
    }
    Ok(format_date(candidate))
}

/// The last completed weekday on or before the given date.
fn last_weekday_on_or_before(date: &str) -> anyhow::Result<String> {
    let parsed = NaiveDate::parse_from_str(date, crate::constants::DATE_FORMAT)
        .with_context(|| format!("invalid comparison date {date}"))?;
    let mut candidate = parsed;
    while matches!(candidate.weekday(), Weekday::Sat | Weekday::Sun) {
        candidate -= Duration::days(1);
    }
    Ok(format_date(candidate))
}
