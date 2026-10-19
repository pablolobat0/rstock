//! The explicitly selected Strategy benchmark.
//!
//! A Strategy benchmark is a reference the portfolio owner chooses on
//! purpose. It is never inferred from the holdings, and selecting one is a
//! reference operation: it creates no transaction, changes no weight, and
//! touches no NAV accounting, inventory, or Effective valuation date.
//!
//! The comparison is calculated from already-ready NAV history and the
//! selected definition's own series, so it stays separate from the
//! exposure-matched primary comparison and can fail without affecting it.

use std::collections::BTreeMap;

use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::constants::{format_date, BASE_CURRENCY, DATE_FORMAT};
use crate::db::repos::{
    benchmark_reference_repo, benchmark_strategy_repo, custom_blend_repo, portfolio_history_repo,
};
use crate::models::{
    BenchmarkComparisonPeriod, BlendComponent, ComparisonAvailability, ComparisonFact,
    DatedWealthPoint, MarketDataLimitation, StrategyComparison, StrategyComparisonPeriod,
    StrategySelection, StrategySelectionOutcome,
};
use crate::services::custom_blends::{self, BlendWindow};
use crate::services::market_data::MarketData;

/// Why a Strategy benchmark is selected: the user chose it explicitly.
pub const SELECTION_BASIS_EXPLICIT: &str = "explicit_user_selection";

/// The role of the selected Strategy comparison.
pub const STRATEGY_ROLE: &str = "strategy";

/// The saved-definition kind of a saved Custom blend in the reference catalog.
const CUSTOM_BLEND_KIND: &str = "custom_blend";

/// The kind of a source-backed reference definition.
const REFERENCE_KIND: &str = "reference";

/// Explicitly selects one saved definition as the portfolio's Strategy
/// benchmark.
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
    let definition = benchmark_reference_repo::find_by_id(db, reference_id).await?;
    anyhow::ensure!(
        is_usable_strategy(&definition),
        "'{reference_id}' is not a usable Strategy benchmark: a Strategy benchmark is a saved Custom blend or a verified source-backed reference"
    );

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

    let replaced = match previous.as_ref() {
        Some(previous) if previous.reference_id != selection.reference_id => {
            let replaced_name = benchmark_reference_repo::find_by_id(db, &previous.reference_id)
                .await
                .ok()
                .map(|definition| definition.display_name);
            (Some(previous.reference_id.clone()), replaced_name)
        }
        _ => (None, None),
    };
    Ok(StrategySelectionOutcome {
        reference_id: Some(selection.reference_id.clone()),
        display_name: Some(definition.display_name),
        selected: true,
        selected_at: Some(selection.selected_at),
        selection_basis: Some(selection.selection_basis),
        replaced_reference_id: replaced.0,
        replaced_display_name: replaced.1,
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
            let name = benchmark_reference_repo::find_by_id(db, &previous.reference_id)
                .await
                .ok()
                .map(|definition| definition.display_name);
            (Some(previous.reference_id.clone()), name)
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
/// intact. Database failures and malformed persisted data remain errors.
pub async fn compute_strategy_comparison(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<StrategyComparison> {
    let Some((selection, definition)) = load_selection(db).await? else {
        return Ok(not_selected());
    };
    let comparison = match prepare_strategy_series(db, market_data, &definition).await {
        Ok(series) => series,
        Err(StrategyPreparation::Database(error)) => return Err(error),
        Err(StrategyPreparation::Coverage(reason)) => {
            return Ok(unavailable(
                &selection,
                &definition,
                &reason,
                Vec::new(),
                None,
            ));
        }
    };
    let Some(boundary) = custom_blends::portfolio_comparison_boundary(db).await? else {
        return Ok(unavailable(
            &selection,
            &definition,
            "the portfolio has no ready complete comparison history, so there is nothing to compare the Strategy benchmark against",
            Vec::new(),
            None,
        ));
    };
    let snapshots = portfolio_history_repo::find_between(
        db,
        &boundary.first_invested_date,
        &boundary.effective_end_date,
    )
    .await?;
    let subject: Vec<(String, f64)> = snapshots
        .iter()
        .map(|snapshot| (snapshot.date.clone(), snapshot.nav))
        .collect();
    Ok(calculate(&selection, &definition, comparison, subject))
}

/// The prepared Strategy benchmark series and its provenance.
struct StrategySeries {
    /// The dated reference values of the selected definition.
    values: Vec<(String, f64)>,
    /// The anchor of the Strategy benchmark itself.
    anchor_date: String,
    /// The kind of the selected definition.
    definition_kind: String,
    /// The reset rule, for a saved Custom blend.
    reset_rule: Option<String>,
    /// The weight convention, for a saved Custom blend.
    weight_convention: Option<String>,
    /// The cash convention, for a saved Custom blend.
    cash_convention: Option<String>,
    /// The component provenance, for a saved Custom blend.
    components: Vec<BlendComponent>,
    /// The honest limitation of the periods the Strategy benchmark cannot
    /// cover.
    earlier_period_limitation: Option<String>,
    /// The market-data limitations of the selected definition.
    limitations: Vec<MarketDataLimitation>,
}

/// The failure of one Strategy benchmark preparation.
enum StrategyPreparation {
    /// A database failure, which remains an error.
    Database(anyhow::Error),
    /// A missing required input, which makes the comparison unavailable.
    Coverage(String),
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

/// Prepares the dated series of the selected Strategy benchmark.
async fn prepare_strategy_series(
    db: &DatabaseConnection,
    market_data: &MarketData,
    definition: &benchmark_reference_repo::BenchmarkReference,
) -> Result<StrategySeries, StrategyPreparation> {
    if custom_blend_repo::exists(db, &definition.id)
        .await
        .map_err(StrategyPreparation::Database)?
    {
        return prepare_blend_series(db, market_data, definition).await;
    }
    prepare_reference_series(db, market_data, definition).await
}

/// Prepares the series of a saved Custom blend selected as a Strategy
/// benchmark.
///
/// The saved definition's own persisted facts are validated first, so
/// malformed persisted data stays an error. The market-data stage afterwards
/// only reports missing coverage.
async fn prepare_blend_series(
    db: &DatabaseConnection,
    market_data: &MarketData,
    definition: &benchmark_reference_repo::BenchmarkReference,
) -> Result<StrategySeries, StrategyPreparation> {
    let blend = custom_blend_repo::find_by_id(db, &definition.id)
        .await
        .map_err(StrategyPreparation::Database)?;
    custom_blends::validate_saved_blend(&blend).map_err(StrategyPreparation::Database)?;
    let start = parse_date(&blend.admitted_start_date).map_err(StrategyPreparation::Database)?;
    let end = market_data.today() - chrono::Duration::days(1);
    if start > end {
        return Err(StrategyPreparation::Database(anyhow::anyhow!(
            "the saved Strategy benchmark '{}' starts on {} but no completed comparison date is available",
            blend.id,
            blend.admitted_start_date
        )));
    }
    let window = BlendWindow::new(start, end);
    let admitted = custom_blends::admit_saved_blend(db, market_data, &blend, &window)
        .await
        .map_err(classify_preparation_failure)?;
    let series = custom_blends::calculate_blend_wealth(&admitted, &window)
        .map_err(classify_preparation_failure)?;
    Ok(StrategySeries {
        values: series
            .wealth
            .iter()
            .map(|point| (point.date.clone(), point.wealth))
            .collect(),
        anchor_date: series.anchor_date,
        definition_kind: CUSTOM_BLEND_KIND.to_owned(),
        reset_rule: Some(blend.reset_rule.clone()),
        weight_convention: Some(blend.weight_convention.clone()),
        cash_convention: Some(blend.cash_convention.clone()),
        components: blend.components.clone(),
        earlier_period_limitation: custom_blends::earlier_period_limitation(&blend),
        limitations: Vec::new(),
    })
}

/// Prepares the series of a source-backed reference selected as a Strategy
/// benchmark.
async fn prepare_reference_series(
    db: &DatabaseConnection,
    market_data: &MarketData,
    definition: &benchmark_reference_repo::BenchmarkReference,
) -> Result<StrategySeries, StrategyPreparation> {
    let start = market_data.today() - chrono::Duration::days(365 * 10);
    let end = market_data.today() - chrono::Duration::days(1);
    let prepared = market_data
        .selected_benchmark_reference_market_data(
            db,
            &definition.id,
            &format_date(start),
            &format_date(end),
        )
        .await
        .map_err(classify_preparation_failure)?;
    let Some(qualification) = prepared.source_qualification.as_ref() else {
        return Err(StrategyPreparation::Coverage(
            "the selected Strategy benchmark has no verified return-basis, adjustment, currency, hedge, and cadence qualification"
                .to_owned(),
        ));
    };
    if qualification.cadence != "daily" {
        return Err(StrategyPreparation::Coverage(format!(
            "the selected Strategy benchmark is not a daily series (cadence: {}); a Strategy comparison needs daily history",
            qualification.cadence
        )));
    }
    let Some(anchor) = prepared
        .genuine_benchmark_series
        .first()
        .map(|(date, _)| date.clone())
    else {
        return Err(StrategyPreparation::Coverage(
            "the selected Strategy benchmark has no completed observations in the comparison window"
                .to_owned(),
        ));
    };
    let earlier_period_limitation = Some(format!(
        "the Strategy benchmark's available history starts on {anchor}; the periods before that date are unavailable for this comparison"
    ));
    Ok(StrategySeries {
        values: prepared.genuine_benchmark_series,
        anchor_date: anchor,
        definition_kind: REFERENCE_KIND.to_owned(),
        reset_rule: None,
        weight_convention: None,
        cash_convention: None,
        components: Vec::new(),
        earlier_period_limitation,
        limitations: prepared.limitations,
    })
}

/// Classifies one preparation failure.
///
/// A database failure stays an error; every other failure is a missing
/// required input that makes the comparison unavailable.
fn classify_preparation_failure(error: anyhow::Error) -> StrategyPreparation {
    if error.downcast_ref::<sea_orm::DbErr>().is_some() {
        StrategyPreparation::Database(error)
    } else {
        StrategyPreparation::Coverage(error.to_string())
    }
}

/// Calculates the aligned Strategy comparison over the common window.
fn calculate(
    selection: &StrategySelection,
    definition: &benchmark_reference_repo::BenchmarkReference,
    series: StrategySeries,
    subject: Vec<(String, f64)>,
) -> StrategyComparison {
    let subject_map: BTreeMap<String, f64> = subject.into_iter().collect();
    let strategy_map: BTreeMap<String, f64> = series.values.into_iter().collect();
    let common: Vec<(String, f64, f64)> = subject_map
        .iter()
        .filter_map(|(date, value)| {
            strategy_map
                .get(date)
                .map(|other| (date.clone(), *value, *other))
        })
        .collect();
    let Some((anchor_date, anchor_value, anchor_strategy)) = common.first().cloned() else {
        return unavailable(
            selection,
            definition,
            "the portfolio and the selected Strategy benchmark have no common completed observation",
            series.limitations,
            series.earlier_period_limitation,
        );
    };
    let Some((end_date, end_value, end_strategy)) = common.last().cloned() else {
        return unavailable(
            selection,
            definition,
            "the portfolio and the selected Strategy benchmark have no common completed observation",
            series.limitations,
            series.earlier_period_limitation,
        );
    };
    if common.len() < 2 {
        return unavailable(
            selection,
            definition,
            "the portfolio and the selected Strategy benchmark have fewer than two common completed observations",
            series.limitations,
            series.earlier_period_limitation,
        );
    }
    let portfolio_wealth: Vec<DatedWealthPoint> = common
        .iter()
        .map(|(date, value, _)| DatedWealthPoint {
            date: date.clone(),
            wealth: value / anchor_value * 100.0,
        })
        .collect();
    let strategy_wealth: Vec<DatedWealthPoint> = common
        .iter()
        .map(|(date, _, other)| DatedWealthPoint {
            date: date.clone(),
            wealth: other / anchor_strategy * 100.0,
        })
        .collect();
    let portfolio_return_pct = (end_value / anchor_value - 1.0) * 100.0;
    let strategy_return_pct = (end_strategy / anchor_strategy - 1.0) * 100.0;

    let subject_series: Vec<(String, f64)> = subject_map.into_iter().collect();
    let strategy_series: Vec<(String, f64)> = strategy_map.into_iter().collect();
    let standard_periods = match crate::services::benchmark::compute_standard_fund_benchmark_periods(
        &subject_series,
        &strategy_series,
        &anchor_date,
        &end_date,
    ) {
        Ok(periods) => periods.into_iter().map(strategy_period).collect(),
        Err(error) => {
            return unavailable(
                selection,
                definition,
                &format!("the standard comparison rows could not be calculated: {error}"),
                series.limitations,
                series.earlier_period_limitation,
            );
        }
    };

    StrategyComparison {
        availability: ComparisonAvailability::Available,
        role: STRATEGY_ROLE.to_owned(),
        reference_id: Some(definition.id.clone()),
        reference_name: Some(definition.display_name.clone()),
        definition_kind: Some(series.definition_kind),
        selected_at: Some(selection.selected_at.clone()),
        selection_basis: Some(selection.selection_basis.clone()),
        currency: BASE_CURRENCY.to_owned(),
        anchor_date: Some(anchor_date.clone()),
        end_date: Some(end_date.clone()),
        strategy_anchor_date: Some(series.anchor_date),
        earlier_period_limitation: series.earlier_period_limitation,
        reset_rule: series.reset_rule,
        weight_convention: series.weight_convention,
        cash_convention: series.cash_convention,
        components: series.components,
        portfolio_wealth,
        strategy_wealth,
        portfolio_return_pct: Some(portfolio_return_pct),
        strategy_return_pct: Some(strategy_return_pct),
        excess_return_percentage_points: Some(portfolio_return_pct - strategy_return_pct),
        standard_periods,
        limitations: series.limitations,
    }
}

/// Maps one shared standard comparison row onto the Strategy comparison.
fn strategy_period(period: crate::models::FundBenchmarkPeriod) -> StrategyComparisonPeriod {
    StrategyComparisonPeriod {
        period: period.period,
        requested_start_date: period.requested_start_date,
        start_date: period.start_date,
        end_date: period.end_date,
        portfolio_return_pct: period.fund_return_pct,
        strategy_return_pct: period.reference_return_pct,
        excess_return_percentage_points: period.excess_return_percentage_points,
        beta: period.beta,
        correlation: period.correlation,
        tracking_error_annualized_pct: period.tracking_error_annualized_pct,
        information_ratio: period.information_ratio,
    }
}

/// The comparison outcome when no Strategy benchmark is selected.
fn not_selected() -> StrategyComparison {
    StrategyComparison {
        availability: ComparisonAvailability::NotApplicable {
            reason: "no Strategy benchmark is explicitly selected".to_owned(),
        },
        role: STRATEGY_ROLE.to_owned(),
        reference_id: None,
        reference_name: None,
        definition_kind: None,
        selected_at: None,
        selection_basis: None,
        currency: BASE_CURRENCY.to_owned(),
        anchor_date: None,
        end_date: None,
        strategy_anchor_date: None,
        earlier_period_limitation: None,
        reset_rule: None,
        weight_convention: None,
        cash_convention: None,
        components: Vec::new(),
        portfolio_wealth: Vec::new(),
        strategy_wealth: Vec::new(),
        portfolio_return_pct: None,
        strategy_return_pct: None,
        excess_return_percentage_points: None,
        standard_periods: standard_periods_unavailable(
            "no Strategy benchmark is explicitly selected",
        ),
        limitations: Vec::new(),
    }
}

/// The comparison outcome when the selected Strategy benchmark cannot be
/// prepared.
fn unavailable(
    selection: &StrategySelection,
    definition: &benchmark_reference_repo::BenchmarkReference,
    reason: &str,
    limitations: Vec<MarketDataLimitation>,
    earlier_period_limitation: Option<String>,
) -> StrategyComparison {
    StrategyComparison {
        availability: ComparisonAvailability::Unavailable {
            reason: reason.to_owned(),
        },
        role: STRATEGY_ROLE.to_owned(),
        reference_id: Some(definition.id.clone()),
        reference_name: Some(definition.display_name.clone()),
        definition_kind: None,
        selected_at: Some(selection.selected_at.clone()),
        selection_basis: Some(selection.selection_basis.clone()),
        currency: BASE_CURRENCY.to_owned(),
        anchor_date: None,
        end_date: None,
        strategy_anchor_date: None,
        earlier_period_limitation,
        reset_rule: None,
        weight_convention: None,
        cash_convention: None,
        components: Vec::new(),
        portfolio_wealth: Vec::new(),
        strategy_wealth: Vec::new(),
        portfolio_return_pct: None,
        strategy_return_pct: None,
        excess_return_percentage_points: None,
        standard_periods: standard_periods_unavailable(reason),
        limitations,
    }
}

/// The standard comparison rows of an unavailable comparison.
fn standard_periods_unavailable(reason: &str) -> Vec<StrategyComparisonPeriod> {
    [
        BenchmarkComparisonPeriod::Ytd,
        BenchmarkComparisonPeriod::OneYear,
        BenchmarkComparisonPeriod::ThreeYear,
        BenchmarkComparisonPeriod::FiveYear,
        BenchmarkComparisonPeriod::SinceComparisonStart,
    ]
    .into_iter()
    .map(|period| StrategyComparisonPeriod {
        period,
        requested_start_date: None,
        start_date: None,
        end_date: None,
        portfolio_return_pct: ComparisonFact::unavailable(reason),
        strategy_return_pct: ComparisonFact::unavailable(reason),
        excess_return_percentage_points: ComparisonFact::unavailable(reason),
        beta: ComparisonFact::unavailable(reason),
        correlation: ComparisonFact::unavailable(reason),
        tracking_error_annualized_pct: ComparisonFact::unavailable(reason),
        information_ratio: ComparisonFact::unavailable(reason),
    })
    .collect()
}

/// Whether a definition can be selected as a Strategy benchmark.
fn is_usable_strategy(definition: &benchmark_reference_repo::BenchmarkReference) -> bool {
    definition.kind == CUSTOM_BLEND_KIND || definition.verification_state == "verified"
}

/// Parses one stored date.
fn parse_date(date: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).with_context(|| format!("malformed date '{date}'"))
}
