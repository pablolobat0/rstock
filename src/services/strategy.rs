//! The explicitly selected Strategy benchmark.
//!
//! A Strategy benchmark is a reference the portfolio owner chooses on
//! purpose. It is never inferred from the holdings, and selecting one is a
//! reference operation: it creates no transaction, changes no weight, and
//! touches no NAV accounting, inventory, or Effective valuation date.
//!
//! The comparison is calculated from the already-ready NAV history and the
//! selected definition's own series, so it stays separate from the
//! exposure-matched primary comparison and can fail without affecting it.

use std::collections::BTreeMap;

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::constants::{format_date, DATE_FORMAT};
use crate::db::repos::{benchmark_reference_repo, benchmark_strategy_repo, custom_blend_repo};
use crate::models::{
    ComparisonAvailability, CustomBlendDefinition, DatedWealthPoint, PortfolioSnapshot,
    StrategyComparison, StrategyProvenance, StrategySelection, StrategySelectionOutcome,
};
use crate::services::custom_blends::{self, BlendWindow};
use crate::services::market_data::MarketData;

/// Why a Strategy benchmark is selected: the user chose it explicitly.
pub const SELECTION_BASIS_EXPLICIT: &str = "explicit_user_selection";

/// The role of the selected Strategy comparison.
pub const STRATEGY_ROLE: &str = "strategy";

/// The calculation method of the selected Strategy comparison.
pub const STRATEGY_METHOD: &str = "explicit_strategy_selection";

/// Explicitly selects one saved definition as the portfolio's Strategy
/// benchmark.
///
/// A Strategy benchmark is a saved Custom blend: a named, immutable definition
/// with explicit target weights, monthly resets, and component provenance.
///
/// The selection is persisted as a whole: the previous selection and the new
/// one are written in one caller-owned transaction, so a failed selection
/// leaves the previous selection untouched. A selection is never inferred from
/// the portfolio's holdings.
pub async fn select_strategy(
    db: &DatabaseConnection,
    market_data: &MarketData,
    reference_id: &str,
) -> anyhow::Result<StrategySelectionOutcome> {
    let reference_id = reference_id.trim();
    anyhow::ensure!(
        !reference_id.is_empty(),
        "a Strategy benchmark needs an identity"
    );
    anyhow::ensure!(
        custom_blend_repo::exists(db, reference_id).await?,
        "'{reference_id}' is not a saved Custom blend; a Strategy benchmark is a saved Custom blend with explicit target weights, monthly resets, and component provenance"
    );
    let definition = benchmark_reference_repo::find_by_id(db, reference_id).await?;

    let selected_at = format!("{}T00:00:00Z", format_date(market_data.today()));
    let selection = StrategySelection {
        reference_id: reference_id.to_owned(),
        selected_at,
        selection_basis: SELECTION_BASIS_EXPLICIT.to_owned(),
    };

    let transaction = db.begin().await?;
    let previous = benchmark_strategy_repo::find_selected(&transaction).await?;
    benchmark_strategy_repo::delete_selection(&transaction).await?;
    benchmark_strategy_repo::insert_selection(&transaction, &selection).await?;
    transaction.commit().await?;

    let (replaced_reference_id, replaced_display_name) = match previous.as_ref() {
        Some(previous) if previous.reference_id != selection.reference_id => {
            let replaced = benchmark_reference_repo::find_by_id(db, &previous.reference_id).await?;
            (
                Some(previous.reference_id.clone()),
                Some(replaced.display_name),
            )
        }
        _ => (None, None),
    };
    Ok(StrategySelectionOutcome {
        reference_id: Some(selection.reference_id.clone()),
        display_name: Some(definition.display_name),
        selected: true,
        selected_at: Some(selection.selected_at),
        selection_basis: Some(selection.selection_basis),
        replaced_reference_id,
        replaced_display_name,
        note: "Strategy benchmark selected explicitly; it is compared independently of the portfolio's actual allocation"
            .to_owned(),
    })
}

/// Explicitly removes the portfolio's Strategy benchmark selection.
///
/// Unselecting when nothing is selected is a no-op rather than an error, so
/// the command stays safe to repeat.
pub async fn unselect_strategy(
    db: &DatabaseConnection,
) -> anyhow::Result<StrategySelectionOutcome> {
    let previous = benchmark_strategy_repo::find_selected(db).await?;
    let transaction = db.begin().await?;
    let removed = benchmark_strategy_repo::delete_selection(&transaction).await?;
    transaction.commit().await?;
    let (replaced_reference_id, replaced_display_name) = match previous.as_ref() {
        Some(previous) => {
            let replaced = benchmark_reference_repo::find_by_id(db, &previous.reference_id).await?;
            (
                Some(previous.reference_id.clone()),
                Some(replaced.display_name),
            )
        }
        None => (None, None),
    };
    Ok(StrategySelectionOutcome {
        reference_id: None,
        display_name: None,
        selected: false,
        selected_at: None,
        selection_basis: None,
        replaced_reference_id,
        replaced_display_name,
        note: if removed {
            "Strategy benchmark unselected; the portfolio is compared only against its exposure-matched primary reference"
                .to_owned()
        } else {
            "No Strategy benchmark was selected".to_owned()
        },
    })
}

/// The currently selected Strategy benchmark, if one is selected.
pub async fn selected_strategy(
    db: &DatabaseConnection,
) -> anyhow::Result<Option<StrategySelection>> {
    benchmark_strategy_repo::find_selected(db).await
}

/// Calculates the selected Strategy comparison against the portfolio.
///
/// The comparison is a reference comparison only. It reads the already-ready
/// NAV history and the selected definition's own series, and it never changes
/// the exposure-matched primary comparison, the portfolio's weights, its NAV
/// accounting, its inventory, or its Effective valuation date.
///
/// A Strategy benchmark that cannot be prepared is reported as an unavailable
/// comparison instead of failing the command, so the primary comparison stays
/// intact. Database failures, corrupt persisted data, and violated invariants
/// remain errors.
pub async fn compute_strategy_comparison(
    db: &DatabaseConnection,
    market_data: &MarketData,
    ready_snapshots: &[PortfolioSnapshot],
) -> anyhow::Result<StrategyComparison> {
    let Some((selection, definition)) = load_selection(db).await? else {
        return Ok(StrategyComparison::not_applicable(
            "no Strategy benchmark is explicitly selected",
        ));
    };
    let blend = custom_blend_repo::find_by_id(db, &definition.id).await?;
    let provenance = provenance_of(&selection, &definition, &blend);
    let series = match prepare_series(db, market_data, &blend).await {
        Ok(series) => series,
        Err(StrategyPreparation::Fatal(error)) => return Err(error),
        Err(StrategyPreparation::Coverage(reason)) => {
            return Ok(StrategyComparison::unavailable(
                reason,
                &provenance,
                Vec::new(),
            ));
        }
    };
    let Some(portfolio_anchor) = ready_snapshots
        .iter()
        .find(|snapshot| snapshot.total_value > crate::constants::FLOAT_EPSILON)
    else {
        return Ok(StrategyComparison::unavailable(
            "the portfolio has no invested Complete NAV snapshot to compare the Strategy benchmark against",
            &provenance,
            Vec::new(),
        ));
    };
    calculate(&provenance, series, ready_snapshots, &portfolio_anchor.date)
}

/// The failure of one Strategy benchmark preparation.
enum StrategyPreparation {
    /// A database failure, corrupt persisted data, or a violated invariant,
    /// which remains an error.
    Fatal(anyhow::Error),
    /// A missing required input, which makes the comparison unavailable.
    Coverage(String),
}

/// The prepared Strategy benchmark series.
struct StrategySeries {
    /// The dated reference values of the selected definition.
    values: Vec<(String, f64)>,
    /// The anchor of the Strategy benchmark itself.
    anchor_date: String,
    /// The honest limitation of the periods the Strategy benchmark cannot
    /// cover, stated by the saved definition.
    earlier_period_limitation: Option<String>,
}

/// Loads the selected Strategy benchmark with its definition.
async fn load_selection(
    db: &DatabaseConnection,
) -> anyhow::Result<
    Option<(
        StrategySelection,
        benchmark_reference_repo::BenchmarkReference,
    )>,
> {
    let Some(selection) = benchmark_strategy_repo::find_selected(db).await? else {
        return Ok(None);
    };
    let definition = benchmark_reference_repo::find_by_id(db, &selection.reference_id).await?;
    Ok(Some((selection, definition)))
}

/// The static provenance of the selected Strategy benchmark.
fn provenance_of(
    selection: &StrategySelection,
    definition: &benchmark_reference_repo::BenchmarkReference,
    blend: &CustomBlendDefinition,
) -> StrategyProvenance {
    StrategyProvenance {
        reference_id: definition.id.clone(),
        reference_name: definition.display_name.clone(),
        definition_kind: custom_blends::CUSTOM_BLEND_KIND.to_owned(),
        reset_rule: Some(blend.reset_rule.clone()),
        weight_convention: Some(blend.weight_convention.clone()),
        cash_convention: Some(blend.cash_convention.clone()),
        components: blend.components.clone(),
        source: definition.source.clone(),
        source_identifier: definition.source_identifier.clone(),
        source_provenance: definition.source_provenance.clone(),
        verification_state: definition.verification_state.clone(),
        selected_at: selection.selected_at.clone(),
        selection_basis: selection.selection_basis.clone(),
    }
}

/// Prepares the dated series of the selected Custom blend.
///
/// The saved definition's own persisted facts are validated first, so a
/// malformed saved definition stays an error. The market-data stage afterwards
/// reports only a missing required observation, and a recorded convention that
/// no longer holds remains a violated invariant.
async fn prepare_series(
    db: &DatabaseConnection,
    market_data: &MarketData,
    blend: &CustomBlendDefinition,
) -> Result<StrategySeries, StrategyPreparation> {
    let start = parse_date(&blend.admitted_start_date).map_err(StrategyPreparation::Fatal)?;
    let end = market_data.today() - Duration::days(1);
    if start > end {
        return Err(StrategyPreparation::Fatal(anyhow::anyhow!(
            "the saved Strategy benchmark '{}' starts on {} but no completed comparison date is available",
            blend.id,
            blend.admitted_start_date
        )));
    }
    let window = BlendWindow::new(start, end);
    let series = custom_blends::prepare_saved_blend_series(db, market_data, blend, &window)
        .await
        .map_err(classify_preparation_failure)?;
    Ok(StrategySeries {
        values: series
            .wealth
            .iter()
            .map(|point| (point.date.clone(), point.wealth))
            .collect(),
        anchor_date: series.anchor_date,
        earlier_period_limitation: series.earlier_period_limitation,
    })
}

/// Classifies one preparation failure.
///
/// A database failure, corrupt persisted data, and a violated invariant stay
/// errors; every other failure is a missing required input that makes the
/// comparison unavailable.
fn classify_preparation_failure(error: anyhow::Error) -> StrategyPreparation {
    if error.downcast_ref::<sea_orm::DbErr>().is_some()
        || error
            .downcast_ref::<custom_blends::BlendConventionViolation>()
            .is_some()
    {
        StrategyPreparation::Fatal(error)
    } else {
        StrategyPreparation::Coverage(error.to_string())
    }
}

/// Calculates the aligned Strategy comparison over the common window.
///
/// Both series are rebased to 100 at the first close they share, so the
/// comparison never measures unmatched endpoints. Every completed weekday of
/// the window must be covered by both series: a missing close leaves the
/// comparison unavailable instead of silently shortening it.
fn calculate(
    provenance: &StrategyProvenance,
    series: StrategySeries,
    ready_snapshots: &[PortfolioSnapshot],
    portfolio_anchor: &str,
) -> anyhow::Result<StrategyComparison> {
    let reference_map: BTreeMap<String, f64> = series.values.into_iter().collect();
    let subject_map: BTreeMap<String, f64> = ready_snapshots
        .iter()
        .map(|snapshot| (snapshot.date.clone(), snapshot.nav))
        .collect();
    let common: Vec<(String, f64, f64)> = subject_map
        .iter()
        .filter_map(|(date, value)| {
            reference_map
                .get(date)
                .map(|other| (date.clone(), *value, *other))
        })
        .collect();
    let Some((anchor_date, anchor_value, anchor_reference)) = common.first().cloned() else {
        return Ok(StrategyComparison::unavailable(
            "the portfolio and the selected Strategy benchmark have no common completed observation",
            provenance,
            Vec::new(),
        ));
    };
    if common.len() < 2 {
        return Ok(StrategyComparison::unavailable(
            "the portfolio and the selected Strategy benchmark have fewer than two common completed observations",
            provenance,
            Vec::new(),
        ));
    }
    let (end_date, end_value, end_reference) = common[common.len() - 1].clone();
    if let Some(reason) =
        missing_weekday_coverage(&subject_map, &reference_map, &anchor_date, &end_date)?
    {
        return Ok(StrategyComparison::unavailable(
            reason,
            provenance,
            Vec::new(),
        ));
    }

    let subject_wealth: Vec<DatedWealthPoint> = common
        .iter()
        .map(|(date, value, _)| DatedWealthPoint {
            date: date.clone(),
            wealth: value / anchor_value * 100.0,
        })
        .collect();
    let reference_wealth: Vec<DatedWealthPoint> = common
        .iter()
        .map(|(date, _, other)| DatedWealthPoint {
            date: date.clone(),
            wealth: other / anchor_reference * 100.0,
        })
        .collect();
    let subject_return_pct = (end_value / anchor_value - 1.0) * 100.0;
    let reference_return_pct = (end_reference / anchor_reference - 1.0) * 100.0;

    let subject_series: Vec<(String, f64)> = subject_wealth
        .iter()
        .map(|point| (point.date.clone(), point.wealth))
        .collect();
    let reference_series: Vec<(String, f64)> = reference_wealth
        .iter()
        .map(|point| (point.date.clone(), point.wealth))
        .collect();
    // The standard rows are measured on the same aligned closes as the graph,
    // and every row exposes its own actual anchor. A trailing window the
    // Strategy benchmark cannot cover stays not applicable instead of being
    // silently shortened.
    let standard_periods = match crate::services::benchmark::compute_standard_benchmark_periods(
        &subject_series,
        &reference_series,
        &anchor_date,
        &end_date,
    ) {
        Ok(periods) => periods,
        Err(error) => {
            return Ok(StrategyComparison::unavailable(
                format!("the standard comparison rows could not be calculated: {error}"),
                provenance,
                Vec::new(),
            ));
        }
    };

    let mut comparison =
        StrategyComparison::selected(provenance, ComparisonAvailability::Available);
    comparison.start_date = Some(anchor_date);
    comparison.end_date = Some(end_date);
    comparison.strategy_anchor_date = Some(series.anchor_date.clone());
    comparison.earlier_period_limitation = earlier_period_limitation(
        series.earlier_period_limitation,
        &series.anchor_date,
        portfolio_anchor,
    );
    comparison.subject_return_pct = Some(subject_return_pct);
    comparison.reference_return_pct = Some(reference_return_pct);
    comparison.excess_return_percentage_points = Some(subject_return_pct - reference_return_pct);
    comparison.subject_wealth = subject_wealth;
    comparison.reference_wealth = reference_wealth;
    comparison.standard_periods = standard_periods;
    Ok(comparison)
}

/// The completed weekday of the comparison window that one of the two series
/// does not cover.
///
/// A missing close leaves the comparison unavailable: it is never dropped
/// from the aligned series, which would silently shorten the measured return.
fn missing_weekday_coverage(
    subject: &BTreeMap<String, f64>,
    reference: &BTreeMap<String, f64>,
    anchor: &str,
    end: &str,
) -> anyhow::Result<Option<String>> {
    let mut day = parse_date(anchor)?;
    let last = parse_date(end)?;
    while day <= last {
        if is_weekday(day) {
            let key = format_date(day);
            if !subject.contains_key(&key) {
                return Ok(Some(format!(
                    "the portfolio has no Complete NAV snapshot on {key}; the Strategy comparison needs every completed weekday of its window"
                )));
            }
            if !reference.contains_key(&key) {
                return Ok(Some(format!(
                    "the selected Strategy benchmark has no observation on {key}; the Strategy comparison needs every completed weekday of its window"
                )));
            }
        }
        day += Duration::days(1);
    }
    Ok(None)
}

/// The honest limitation of the periods before the comparison anchor.
///
/// A Strategy benchmark that starts after the portfolio cannot cover the
/// earlier periods, and that limitation is stated instead of the window
/// being silently shortened. A Strategy benchmark that already covers the
/// portfolio's start has no earlier-period limitation.
fn earlier_period_limitation(
    limitation: Option<String>,
    strategy_anchor: &str,
    portfolio_anchor: &str,
) -> Option<String> {
    if strategy_anchor <= portfolio_anchor {
        return None;
    }
    Some(limitation.unwrap_or_else(|| {
        format!(
            "the Strategy benchmark's history starts on {strategy_anchor}; the periods before that date are unavailable for this comparison"
        )
    }))
}

/// Whether a date is a completed weekday.
fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Parses one stored date.
fn parse_date(date: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).with_context(|| format!("malformed date '{date}'"))
}
