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
}

impl FundBenchmarkComparison {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            availability: ComparisonAvailability::Unavailable {
                reason: reason.into(),
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
    pub candidate_correlation: CandidateCorrelationResult,
    pub holdings_changed: bool,
    pub last_snapshot_date: Option<String>,
    pub holding_diff: Vec<HoldingChange>,
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
