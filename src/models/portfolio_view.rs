use crate::models::{
    FactAvailability, MarketDataLimitation, PortfolioInventory, PortfolioSnapshot,
};

/// Presentation-neutral request for the ready NAV history the composer must
/// include in the outcome. Callers choose a period; the Portfolio view
/// composer owns the period date calculation and the ready-history read, so
/// presentation never computes dates or queries history itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavHistoryRequest {
    OneMonth,
    ThreeMonths,
    SixMonths,
    Ytd,
    OneYear,
    ThreeYears,
    FiveYears,
    All,
}

/// The core Portfolio performance state of one Portfolio view. Available,
/// unavailable, and not applicable are explicit outcome states rather than
/// nullable placeholders over shared fields.
#[derive(Debug, Clone, PartialEq)]
pub enum PortfolioPerformance {
    /// A Complete NAV snapshot exists, so performance is measurable at its
    /// Effective valuation date even when limitations prevent advancing that
    /// date.
    Available(Box<AvailablePortfolioPerformance>),
    /// Performance holdings exist but no Complete NAV snapshot can be
    /// produced. The readiness limitations of that blocked initial NAV
    /// remain visible without inventing measured facts.
    Unavailable {
        nav_history_limitations: Vec<MarketDataLimitation>,
    },
    /// There are no current or historical performance holdings to measure.
    NotApplicable,
}

/// One period's cohesive return and risk facts. Every calculated fact keeps
/// its independent availability: insufficient period history and
/// mathematically undefined metrics are not applicable, while a missing
/// required input makes only the dependent fact unavailable. Period reference
/// snapshot dates remain calculation details and are not exposed as facts.
///
/// All means the whole measurable lifetime since inception and carries the
/// same facts as every other period.
#[derive(Debug, Clone, PartialEq)]
pub struct PeriodOutcome {
    /// Return over the period as a percentage of the baseline NAV. YTD and 1Y
    /// are simple interval returns; 3Y, 5Y, and All are annualized CAGR
    /// values consistent with the existing period conventions.
    pub return_pct: FactAvailability<f64>,
    /// Annualized volatility of the period's weekday NAV log returns.
    pub volatility: FactAvailability<f64>,
    /// Worst peak-to-trough decline of the period's NAV series, as a
    /// negative percentage.
    pub max_drawdown: FactAvailability<f64>,
    /// Beta against the configured benchmark, using the period's NAV
    /// returns aligned with benchmark observations.
    pub beta: FactAvailability<f64>,
    pub sharpe: FactAvailability<f64>,
    pub sortino: FactAvailability<f64>,
}

/// The YTD, 1Y, 3Y, 5Y, and All period outcomes of available Portfolio
/// performance. Available performance always contains all five outcomes;
/// individual facts inside them still express their own availability.
#[derive(Debug, Clone, PartialEq)]
pub struct PerformancePeriods {
    pub ytd: PeriodOutcome,
    pub one_year: PeriodOutcome,
    pub three_years: PeriodOutcome,
    pub five_years: PeriodOutcome,
    /// Since inception.
    pub all: PeriodOutcome,
}

/// The facts of an available Portfolio performance state. A Complete NAV
/// snapshot guarantees the synchronized facts and all five period outcomes,
/// so none of them is optional: an available state never carries nullable
/// placeholders, while each calculated period fact keeps independent
/// availability.
///
/// Synchronized facts describe the portfolio at one shared Effective
/// valuation date. They exclude Monetary holdings and stay distinct from the
/// mixed-date current inventory values, which use Individual prices and live
/// quotes inside `[PortfolioInventory]`.
#[derive(Debug, Clone, PartialEq)]
pub struct AvailablePortfolioPerformance {
    /// The latest Complete NAV snapshot date after readiness: the last date
    /// in the contiguous calendar-date range for which NAV can be
    /// calculated.
    pub effective_valuation_date: String,
    /// Performance-holdings value synchronized with the Effective valuation
    /// date. It follows the same scope as NAV and returns: performance
    /// holdings valued at that date plus the dividend income accumulated
    /// through it, with Monetary holdings excluded. It stays distinct from
    /// the mixed-date current inventory values.
    pub synchronized_value: f64,
    pub nav: f64,
    /// The earliest Complete NAV snapshot date, bounding the measurable
    /// lifetime of the portfolio.
    pub inception_date: String,
    /// The YTD, 1Y, 3Y, 5Y, and All period outcomes measured over the ready
    /// NAV history through the Effective valuation date.
    pub periods: PerformancePeriods,
    /// NAV/history readiness Market data limitations. They are independent
    /// from fact availability and may coexist with this older snapshot.
    pub nav_history_limitations: Vec<MarketDataLimitation>,
    /// Market data limitations scoped to benchmark-dependent risk facts
    /// (beta). They are independent from every other limitation scope and
    /// never imply that NAV, inventory, or portfolio-only risk facts are
    /// invalid.
    pub benchmark_risk_limitations: Vec<MarketDataLimitation>,
}

/// One complete, presentation-neutral Portfolio view outcome. It is composed
/// from the always-present Portfolio inventory and an explicit core
/// Portfolio performance state, and includes the requested ready NAV history
/// in the same successful outcome.
///
/// The outcome is derived from the Transaction ledger, market data, and
/// existing NAV history on every request; it is never persisted. Expected
/// domain absence appears in the outcome states, while database failures,
/// malformed persisted data, invariant violations, and required-lookup
/// failures remain errors and produce no partial Portfolio view.
#[derive(Debug)]
pub struct PortfolioView {
    /// Current Transaction ledger holdings with separately owned
    /// performance-holding and Monetary-holding sections. Always present,
    /// including for empty, future-only, Monetary-only, and blocked
    /// performance scenarios.
    pub inventory: PortfolioInventory,
    /// Core Portfolio performance state derived from NAV readiness and the
    /// measured performance-holding scope.
    pub performance: PortfolioPerformance,
    /// Requested ready NAV history for the accepted `NavHistoryRequest`,
    /// obtained before the outcome is returned so a later history read can
    /// never split a successful presentation. Empty while no Complete NAV
    /// snapshot exists.
    pub nav_history: Vec<PortfolioSnapshot>,
}
