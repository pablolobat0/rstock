use serde::Serialize;

use crate::models::{
    BenchmarkComparisonPeriod, BlendComponent, ComparisonAvailability, ComparisonFact,
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

/// One standard comparison row of the selected Strategy comparison.
///
/// The rows answer the strategy question and stay separate from the
/// exposure-matched primary comparison: the subject is the portfolio and the
/// reference is the selected Strategy benchmark.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StrategyComparisonPeriod {
    pub period: BenchmarkComparisonPeriod,
    /// The trailing or calendar anchor the row asked for.
    pub requested_start_date: Option<String>,
    /// The actual anchor the row was calculated from.
    pub start_date: Option<String>,
    /// The actual end the row was calculated to.
    pub end_date: Option<String>,
    /// The portfolio's return over the row.
    pub portfolio_return_pct: ComparisonFact<f64>,
    /// The selected Strategy benchmark's return over the row.
    pub strategy_return_pct: ComparisonFact<f64>,
    /// The portfolio's return minus the Strategy benchmark's return, in
    /// percentage points.
    pub excess_return_percentage_points: ComparisonFact<f64>,
    pub beta: ComparisonFact<f64>,
    pub correlation: ComparisonFact<f64>,
    /// Sample deviation of arithmetic active returns, annualized and expressed
    /// in percent.
    pub tracking_error_annualized_pct: ComparisonFact<f64>,
    /// Annualized information ratio from arithmetic daily active returns.
    pub information_ratio: ComparisonFact<f64>,
}

/// The complete outcome of the selected Strategy comparison.
///
/// The outcome is a reference comparison only: it never changes the
/// exposure-matched primary comparison, the portfolio's weights, its NAV
/// accounting, its inventory, or its Effective valuation date.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StrategyComparison {
    /// The outcome state of the whole comparison.
    pub availability: ComparisonAvailability,
    /// The role of this comparison; it is always the strategy role.
    pub role: String,
    /// The selected definition identity.
    pub reference_id: Option<String>,
    /// The selected definition's display name.
    pub reference_name: Option<String>,
    /// The kind of the selected definition: a saved Custom blend or a
    /// source-backed reference.
    pub definition_kind: Option<String>,
    /// When the Strategy benchmark was explicitly selected.
    pub selected_at: Option<String>,
    /// Why the Strategy benchmark is selected; the selection is always
    /// explicit.
    pub selection_basis: Option<String>,
    /// The comparison currency; EUR is the Base currency.
    pub currency: String,
    /// The anchor the comparison actually starts at. A Strategy benchmark that
    /// starts later than the portfolio exposes its own anchor here.
    pub anchor_date: Option<String>,
    /// The last close the comparison reaches.
    pub end_date: Option<String>,
    /// The anchor of the Strategy benchmark itself, before it is aligned with
    /// the portfolio.
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
    /// The portfolio's rebased wealth over the comparison window.
    pub portfolio_wealth: Vec<DatedWealthPoint>,
    /// The selected Strategy benchmark's rebased wealth over the comparison
    /// window.
    pub strategy_wealth: Vec<DatedWealthPoint>,
    /// The portfolio's cumulative return over the comparison window.
    pub portfolio_return_pct: Option<f64>,
    /// The Strategy benchmark's cumulative return over the comparison window.
    pub strategy_return_pct: Option<f64>,
    /// The difference between the two cumulative returns, in percentage
    /// points.
    pub excess_return_percentage_points: Option<f64>,
    /// The standard comparison rows, calculated independently of the graph
    /// period.
    pub standard_periods: Vec<StrategyComparisonPeriod>,
    /// The market-data limitations of the selected definition.
    pub limitations: Vec<MarketDataLimitation>,
}
