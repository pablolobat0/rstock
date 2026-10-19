use serde::Serialize;

use crate::models::{
    BenchmarkPeriod, BlendComponent, ComparisonAvailability, DatedWealthPoint, MarketDataLimitation,
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
    /// The reset rule of the selected definition, for a saved Custom blend.
    pub reset_rule: Option<String>,
    /// The weight convention of the selected definition, for a saved Custom
    /// blend.
    pub weight_convention: Option<String>,
    /// The cash convention of the selected definition, for a saved Custom
    /// blend.
    pub cash_convention: Option<String>,
    /// The component provenance of the selected definition, for a saved Custom
    /// blend.
    pub components: Vec<BlendComponent>,
    /// The kind of the selected definition: a saved Custom blend or a
    /// source-backed reference.
    pub definition_kind: Option<String>,
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
    /// The comparison outcome for a portfolio that carries no selected
    /// Strategy benchmark.
    pub fn not_applicable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        let mut comparison = Self::empty(ComparisonAvailability::NotApplicable {
            reason: reason.clone(),
        });
        comparison.standard_periods = crate::models::BenchmarkComparisonPeriod::standard_periods()
            .into_iter()
            .map(|period| BenchmarkPeriod::not_applicable(period, reason.clone()))
            .collect();
        comparison
    }

    /// The comparison outcome when the selected Strategy benchmark cannot be
    /// prepared.
    ///
    /// The selected definition and its Market data limitations stay visible so
    /// the reason is inspectable instead of hidden behind a summary.
    pub fn unavailable(
        reason: impl Into<String>,
        reference_label: impl Into<String>,
        limitations: Vec<MarketDataLimitation>,
    ) -> Self {
        let reason = reason.into();
        let mut comparison = Self::empty(ComparisonAvailability::Unavailable {
            reason: reason.clone(),
        });
        comparison.reference_label = reference_label.into();
        comparison.limitations = limitations;
        comparison.standard_periods = crate::models::BenchmarkComparisonPeriod::standard_periods()
            .into_iter()
            .map(|period| BenchmarkPeriod::unavailable(period, reason.clone()))
            .collect();
        comparison
    }

    fn empty(availability: ComparisonAvailability) -> Self {
        let reason = match &availability {
            ComparisonAvailability::Available => String::new(),
            ComparisonAvailability::Unavailable { reason }
            | ComparisonAvailability::NotApplicable { reason } => reason.clone(),
        };
        Self {
            availability,
            role: "strategy".to_owned(),
            method: "explicit_strategy_selection".to_owned(),
            subject_label: "Portfolio".to_owned(),
            reference_label: "Strategy benchmark".to_owned(),
            currency: crate::constants::BASE_CURRENCY.to_owned(),
            start_date: None,
            end_date: None,
            strategy_anchor_date: None,
            earlier_period_limitation: None,
            reset_rule: None,
            weight_convention: None,
            cash_convention: None,
            components: Vec::new(),
            definition_kind: None,
            selected_at: None,
            selection_basis: None,
            subject_return_pct: None,
            reference_return_pct: None,
            excess_return_percentage_points: None,
            subject_wealth: Vec::new(),
            reference_wealth: Vec::new(),
            standard_periods: crate::models::BenchmarkComparisonPeriod::standard_periods()
                .into_iter()
                .map(|period| BenchmarkPeriod::unavailable(period, reason.clone()))
                .collect(),
            limitations: Vec::new(),
        }
    }
}
