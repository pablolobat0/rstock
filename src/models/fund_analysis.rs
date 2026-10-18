use super::portfolio::{AllocationEntry, BenchmarkReferenceInfo, FundHolding};
use super::MarketDataLimitation;
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum ComparisonAvailability {
    Available,
    Unavailable { reason: String },
    NotApplicable { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComparisonCoverageIssue {
    MissingAnchor(String),
    MissingEnd(String),
    InternalWeekdayGap(String),
    InsufficientCommonObservations,
}

impl std::fmt::Display for ComparisonCoverageIssue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingAnchor(date) => write!(
                formatter,
                "required comparison anchor {date} is not covered by both series"
            ),
            Self::MissingEnd(date) => write!(
                formatter,
                "required comparison end {date} is not covered by both series"
            ),
            Self::InternalWeekdayGap(date) => write!(
                formatter,
                "required internal weekday observation missing on {date}"
            ),
            Self::InsufficientCommonObservations => write!(
                formatter,
                "fund and primary reference need at least two common EUR observations"
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DatedWealthPoint {
    pub date: String,
    pub wealth: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FundBenchmarkComparison {
    pub availability: ComparisonAvailability,
    pub role: String,
    pub reference_id: Option<String>,
    pub reference_name: Option<String>,
    /// Currency of the persisted reference prices before conversion.
    pub native_currency: Option<String>,
    /// Currency of both wealth series and returns.
    pub currency: String,
    pub cadence: Option<String>,
    pub return_basis: Option<String>,
    pub adjustment: Option<String>,
    pub hedge_policy: Option<String>,
    pub source: Option<String>,
    pub source_identifier: Option<String>,
    pub source_provenance: Option<String>,
    pub fund_limitations: Vec<MarketDataLimitation>,
    pub reference_limitations: Vec<MarketDataLimitation>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub fund_return_pct: Option<f64>,
    pub reference_return_pct: Option<f64>,
    pub excess_return_percentage_points: Option<f64>,
    pub fund_wealth: Vec<DatedWealthPoint>,
    pub reference_wealth: Vec<DatedWealthPoint>,
    pub standard_periods: Vec<FundBenchmarkPeriod>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchmarkComparisonPeriod {
    Ytd,
    OneYear,
    ThreeYear,
    FiveYear,
    SinceComparisonStart,
}

impl BenchmarkComparisonPeriod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ytd => "YTD",
            Self::OneYear => "1Y",
            Self::ThreeYear => "3Y",
            Self::FiveYear => "5Y",
            Self::SinceComparisonStart => "Since Comparison Start",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ComparisonFact<T> {
    Available { value: T },
    Unavailable { reason: String },
    NotApplicable { reason: String },
}

impl<T> ComparisonFact<T> {
    pub fn available(value: T) -> Self {
        Self::Available { value }
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn not_applicable(reason: impl Into<String>) -> Self {
        Self::NotApplicable {
            reason: reason.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FundBenchmarkPeriod {
    pub period: BenchmarkComparisonPeriod,
    pub requested_start_date: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub fund_return_pct: ComparisonFact<f64>,
    pub reference_return_pct: ComparisonFact<f64>,
    pub excess_return_percentage_points: ComparisonFact<f64>,
    pub beta: ComparisonFact<f64>,
    pub correlation: ComparisonFact<f64>,
    /// Sample deviation of arithmetic active returns, annualized and expressed in percent.
    pub tracking_error_annualized_pct: ComparisonFact<f64>,
    /// Annualized information ratio from arithmetic daily active returns.
    pub information_ratio: ComparisonFact<f64>,
}

impl FundBenchmarkComparison {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            availability: ComparisonAvailability::Unavailable {
                reason: reason.clone(),
            },
            role: "primary".to_owned(),
            reference_id: None,
            reference_name: None,
            native_currency: None,
            currency: "EUR".to_owned(),
            cadence: None,
            return_basis: None,
            adjustment: None,
            hedge_policy: None,
            source: None,
            source_identifier: None,
            source_provenance: None,
            fund_limitations: Vec::new(),
            reference_limitations: Vec::new(),
            start_date: None,
            end_date: None,
            fund_return_pct: None,
            reference_return_pct: None,
            excess_return_percentage_points: None,
            fund_wealth: Vec::new(),
            reference_wealth: Vec::new(),
            standard_periods: [
                BenchmarkComparisonPeriod::Ytd,
                BenchmarkComparisonPeriod::OneYear,
                BenchmarkComparisonPeriod::ThreeYear,
                BenchmarkComparisonPeriod::FiveYear,
                BenchmarkComparisonPeriod::SinceComparisonStart,
            ]
            .into_iter()
            .map(|period| FundBenchmarkPeriod::unavailable(period, reason.clone()))
            .collect(),
        }
    }
}

impl FundBenchmarkPeriod {
    pub fn not_applicable(period: BenchmarkComparisonPeriod, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            period,
            requested_start_date: None,
            start_date: None,
            end_date: None,
            fund_return_pct: ComparisonFact::not_applicable(reason.clone()),
            reference_return_pct: ComparisonFact::not_applicable(reason.clone()),
            excess_return_percentage_points: ComparisonFact::not_applicable(reason.clone()),
            beta: ComparisonFact::not_applicable(reason.clone()),
            correlation: ComparisonFact::not_applicable(reason.clone()),
            tracking_error_annualized_pct: ComparisonFact::not_applicable(reason.clone()),
            information_ratio: ComparisonFact::not_applicable(reason),
        }
    }

    pub fn unavailable(period: BenchmarkComparisonPeriod, reason: impl Into<String>) -> Self {
        let reason = reason.into();
        Self {
            period,
            requested_start_date: None,
            start_date: None,
            end_date: None,
            fund_return_pct: ComparisonFact::unavailable(reason.clone()),
            reference_return_pct: ComparisonFact::unavailable(reason.clone()),
            excess_return_percentage_points: ComparisonFact::unavailable(reason.clone()),
            beta: ComparisonFact::unavailable(reason.clone()),
            correlation: ComparisonFact::unavailable(reason.clone()),
            tracking_error_annualized_pct: ComparisonFact::unavailable(reason.clone()),
            information_ratio: ComparisonFact::unavailable(reason),
        }
    }
}

#[derive(Serialize)]
pub struct FundAnalysisResult {
    pub ms_code: String,
    pub name: Option<String>,
    pub fund_currency: Option<String>,
    pub aum: Option<f64>,
    pub aum_currency: Option<String>,
    pub inception_date: Option<String>,
    pub total_holdings: Option<i32>,
    pub portfolio_date: Option<String>,
    pub top_10_weight: Option<f64>,
    pub top_holdings: Vec<FundHolding>,
    pub sector_breakdown: Vec<AllocationEntry>,
    pub country_breakdown: Vec<AllocationEntry>,
    pub currency_breakdown: Vec<AllocationEntry>,
    pub ytd: Option<FundPeriodMetrics>,
    pub one_year: Option<FundPeriodMetrics>,
    pub three_year: Option<FundPeriodMetrics>,
    pub five_year: Option<FundPeriodMetrics>,
    pub all_time: Option<FundPeriodMetrics>,
    pub benchmark_reference: BenchmarkReferenceInfo,
    pub benchmark_limitations: Vec<MarketDataLimitation>,
    pub primary_benchmark_comparison: FundBenchmarkComparison,
    pub risk_free_assumption: RiskFreeAssumption,
    pub candidate_correlation: CandidateCorrelationResult,
    pub holdings_changed: bool,
    pub last_snapshot_date: Option<String>,
    pub holding_diff: Vec<HoldingChange>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RiskFreeAssumption {
    pub annual_rate_pct: f64,
    pub applies_to: Vec<String>,
    pub not_applied_to: Vec<String>,
}

impl RiskFreeAssumption {
    pub fn current() -> Self {
        Self {
            annual_rate_pct: crate::constants::ANNUAL_RISK_FREE_RATE * 100.0,
            applies_to: vec!["Sharpe".to_owned(), "Sortino".to_owned()],
            not_applied_to: vec!["€STR".to_owned(), "retained cash return".to_owned()],
        }
    }
}

#[derive(Serialize)]
pub struct CandidateCorrelationResult {
    pub period_label: String,
    pub rows: Vec<CandidateCorrelationRow>,
}

pub struct CandidateCorrelationPeriod {
    pub label: &'static str,
    pub days: i64,
}

#[derive(Serialize)]
pub struct CandidateCorrelationRow {
    pub label: String,
    pub correlation: Option<f64>,
    pub reason: Option<String>,
    pub is_portfolio: bool,
}

#[derive(Serialize)]
pub struct FundPeriodMetrics {
    pub total_return: f64,
    pub cagr: Option<f64>,
    pub volatility: Option<f64>,
    pub sharpe: Option<f64>,
    pub sortino: Option<f64>,
    pub max_drawdown: Option<f64>,
    pub beta: Option<f64>,
}

#[derive(Serialize)]
pub struct HoldingChange {
    pub name: String,
    pub change_type: HoldingChangeType,
    pub old_weight: Option<f64>,
    pub new_weight: Option<f64>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldingChangeType {
    Added,
    Removed,
    WeightChanged,
}

#[derive(Clone)]
pub struct FundData {
    pub fund_currency: Option<String>,
    pub total_holdings: Option<i32>,
    pub portfolio_date: Option<String>,
    pub holdings: Vec<FundHolding>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FundQuoteMetadata {
    pub name: Option<String>,
    pub aum: Option<f64>,
    pub aum_currency: Option<String>,
    pub inception_date: Option<String>,
    pub quote_currency: Option<String>,
}
