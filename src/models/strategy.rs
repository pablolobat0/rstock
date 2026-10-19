use serde::Serialize;

use crate::db::entities::benchmark_strategy_selection;
use crate::models::{
    BenchmarkComparisonPeriod, BenchmarkPeriod, BlendComponent, ComparisonAvailability,
    DatedWealthPoint, MarketDataLimitation,
};

/// The explicitly selected Strategy benchmark.
///
/// A selection records only which saved definition the portfolio is compared
/// against. It never carries a target allocation, a weight, or anything else
/// derived from the portfolio's holdings.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StrategySelection {
    /// The selected benchmark definition identity.
    pub reference_id: String,
    /// When the selection was made.
    pub selected_at: String,
    /// Why the definition is selected; the selection is always explicit.
    pub selection_basis: String,
}

impl From<benchmark_strategy_selection::Model> for StrategySelection {
    fn from(model: benchmark_strategy_selection::Model) -> Self {
        Self {
            reference_id: model.reference_id,
            selected_at: model.selected_at,
            selection_basis: model.selection_basis,
        }
    }
}

/// The receipt of one explicit Strategy selection or unselection.
///
/// The receipt states what is selected afterwards, so the human and JSON
/// surfaces report the same selection state.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StrategySelectionOutcome {
    /// The selected definition identity after the command, if one is selected.
    pub reference_id: Option<String>,
    /// The selected definition's display name, if one is selected.
    pub display_name: Option<String>,
    /// Whether a Strategy benchmark is selected after the command.
    pub selected: bool,
    /// When the current selection was made.
    pub selected_at: Option<String>,
    /// Why the current selection was made.
    pub selection_basis: Option<String>,
    /// The definition that was replaced, if the command replaced one.
    pub replaced_reference_id: Option<String>,
    /// The definition that was replaced, by display name.
    pub replaced_display_name: Option<String>,
    /// A human-readable statement of what the command did.
    pub note: String,
}

/// The static provenance of the selected Strategy benchmark.
///
/// The provenance stays visible whether the comparison could be measured or
/// not: the selected definition, the conventions it was admitted with, its
/// components, and the explicit choice that selected it are the meaning of
/// the comparison.
#[derive(Clone, Debug, PartialEq)]
pub struct StrategyProvenance {
    /// The selected benchmark definition identity.
    pub reference_id: String,
    /// The selected benchmark definition's display name.
    pub reference_name: String,
    /// The kind of the selected definition: a saved Custom blend.
    pub definition_kind: String,
    /// The reset rule of the selected definition.
    pub reset_rule: Option<String>,
    /// The weight convention of the selected definition.
    pub weight_convention: Option<String>,
    /// The cash convention of the selected definition.
    pub cash_convention: Option<String>,
    /// The component membership and conventions of the selected definition.
    pub components: Vec<BlendComponent>,
    /// The source of the selected definition.
    pub source: String,
    /// The source lookup identity of the selected definition.
    pub source_identifier: String,
    /// The source provenance of the selected definition.
    pub source_provenance: String,
    /// The verification state of the selected definition.
    pub verification_state: String,
    /// When the Strategy benchmark was explicitly selected.
    pub selected_at: String,
    /// Why the Strategy benchmark is selected; the selection is always
    /// explicit.
    pub selection_basis: String,
}

/// The comparison of the portfolio against its explicitly selected Strategy
/// benchmark.
///
/// The comparison is a reference comparison only: it answers a different
/// question from the exposure-matched primary comparison and never changes
/// that comparison, the portfolio's weights, its NAV accounting, its
/// inventory, or its Effective valuation date.
///
/// The outcome keeps its own availability: an expected missing input is an
/// explicit unavailable state, while a database failure, a corrupt persisted
/// observation, or a violated invariant remains an error.
#[derive(Clone, Debug, PartialEq)]
pub struct StrategyComparison {
    pub availability: ComparisonAvailability,
    /// The comparison role: the explicitly selected Strategy benchmark.
    pub role: String,
    /// The calculation method of this comparison.
    pub method: String,
    /// The subject of the comparison: the portfolio itself.
    pub subject_label: String,
    /// The reference of the comparison: the selected Strategy benchmark.
    pub reference_label: String,
    /// The currency of both compared series and returns.
    pub currency: String,
    /// The anchor the comparison starts at, where both series reach 100.
    pub start_date: Option<String>,
    /// The last close the comparison reaches.
    pub end_date: Option<String>,
    /// The anchor of the Strategy benchmark itself, before it is aligned with
    /// the portfolio. A Strategy benchmark that starts later than the
    /// portfolio exposes its own anchor here.
    pub strategy_anchor_date: Option<String>,
    /// The honest limitation of the periods the Strategy benchmark cannot
    /// cover.
    pub earlier_period_limitation: Option<String>,
    /// The reset rule of the selected definition.
    pub reset_rule: Option<String>,
    /// The weight convention of the selected definition.
    pub weight_convention: Option<String>,
    /// The cash convention of the selected definition.
    pub cash_convention: Option<String>,
    /// The component provenance of the selected definition.
    pub components: Vec<BlendComponent>,
    /// The kind of the selected definition: a saved Custom blend.
    pub definition_kind: Option<String>,
    /// The selected benchmark definition identity.
    pub reference_id: Option<String>,
    /// The source of the selected definition.
    pub source: Option<String>,
    /// The source lookup identity of the selected definition.
    pub source_identifier: Option<String>,
    /// The source provenance of the selected definition.
    pub source_provenance: Option<String>,
    /// The verification state of the selected definition.
    pub verification_state: Option<String>,
    /// When the Strategy benchmark was explicitly selected.
    pub selected_at: Option<String>,
    /// Why the Strategy benchmark is selected; the selection is always
    /// explicit.
    pub selection_basis: Option<String>,
    pub subject_return_pct: Option<f64>,
    pub reference_return_pct: Option<f64>,
    pub excess_return_percentage_points: Option<f64>,
    /// The portfolio NAV rebased to 100 at the comparison anchor.
    pub subject_wealth: Vec<DatedWealthPoint>,
    /// The selected Strategy benchmark rebased to 100 at the comparison
    /// anchor.
    pub reference_wealth: Vec<DatedWealthPoint>,
    /// The YTD, 1Y, 3Y, 5Y, and since-comparison-start rows, independent of
    /// the graph period.
    pub standard_periods: Vec<BenchmarkPeriod>,
    /// The Market data limitations of the selected definition.
    pub limitations: Vec<MarketDataLimitation>,
}

impl StrategyComparison {
    /// The comparison of a selected Strategy benchmark.
    ///
    /// The selected definition and its provenance stay visible before the
    /// comparison is measured, so an unavailable comparison still explains
    /// what was selected.
    pub fn selected(provenance: &StrategyProvenance, availability: ComparisonAvailability) -> Self {
        let standard_periods = match &availability {
            ComparisonAvailability::Available => BenchmarkComparisonPeriod::standard_periods()
                .into_iter()
                .map(|period| {
                    BenchmarkPeriod::unavailable(period, "the comparison is not measured")
                })
                .collect(),
            ComparisonAvailability::Unavailable { reason } => {
                BenchmarkComparisonPeriod::standard_periods()
                    .into_iter()
                    .map(|period| BenchmarkPeriod::unavailable(period, reason.clone()))
                    .collect()
            }
            ComparisonAvailability::NotApplicable { reason } => {
                BenchmarkComparisonPeriod::standard_periods()
                    .into_iter()
                    .map(|period| BenchmarkPeriod::not_applicable(period, reason.clone()))
                    .collect()
            }
        };
        Self {
            availability,
            role: "strategy".to_owned(),
            method: "explicit_strategy_selection".to_owned(),
            subject_label: "Portfolio".to_owned(),
            reference_label: provenance.reference_name.clone(),
            currency: crate::constants::BASE_CURRENCY.to_owned(),
            start_date: None,
            end_date: None,
            strategy_anchor_date: None,
            earlier_period_limitation: None,
            reset_rule: provenance.reset_rule.clone(),
            weight_convention: provenance.weight_convention.clone(),
            cash_convention: provenance.cash_convention.clone(),
            components: provenance.components.clone(),
            definition_kind: Some(provenance.definition_kind.clone()),
            reference_id: Some(provenance.reference_id.clone()),
            source: Some(provenance.source.clone()),
            source_identifier: Some(provenance.source_identifier.clone()),
            source_provenance: Some(provenance.source_provenance.clone()),
            verification_state: Some(provenance.verification_state.clone()),
            selected_at: Some(provenance.selected_at.clone()),
            selection_basis: Some(provenance.selection_basis.clone()),
            subject_return_pct: None,
            reference_return_pct: None,
            excess_return_percentage_points: None,
            subject_wealth: Vec::new(),
            reference_wealth: Vec::new(),
            standard_periods,
            limitations: Vec::new(),
        }
    }

    /// The comparison outcome for a portfolio that carries no selected
    /// Strategy benchmark.
    pub fn not_applicable(reason: impl Into<String>) -> Self {
        Self::selected(
            &StrategyProvenance::empty(),
            ComparisonAvailability::NotApplicable {
                reason: reason.into(),
            },
        )
    }

    /// The comparison outcome when the selected Strategy benchmark cannot be
    /// prepared.
    ///
    /// The selected definition, its provenance, and its Market data
    /// limitations stay visible so the reason is inspectable instead of
    /// hidden behind a summary.
    pub fn unavailable(
        reason: impl Into<String>,
        provenance: &StrategyProvenance,
        limitations: Vec<MarketDataLimitation>,
    ) -> Self {
        let reason = reason.into();
        let mut comparison = Self::selected(
            provenance,
            ComparisonAvailability::Unavailable {
                reason: reason.clone(),
            },
        );
        comparison.limitations = limitations;
        comparison
    }
}

impl StrategyProvenance {
    /// The provenance of a portfolio that carries no selected Strategy
    /// benchmark.
    fn empty() -> Self {
        Self {
            reference_id: String::new(),
            reference_name: "Strategy benchmark".to_owned(),
            definition_kind: String::new(),
            reset_rule: None,
            weight_convention: None,
            cash_convention: None,
            components: Vec::new(),
            source: String::new(),
            source_identifier: String::new(),
            source_provenance: String::new(),
            verification_state: String::new(),
            selected_at: String::new(),
            selection_basis: String::new(),
        }
    }
}
