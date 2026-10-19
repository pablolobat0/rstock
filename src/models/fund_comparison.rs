use super::{FundBenchmarkComparison, FundPeriodMetrics, RiskFreeAssumption};
use serde::Serialize;

#[derive(Serialize)]
pub struct FundComparisonResult {
    pub fund_a: FundComparisonSide,
    pub fund_b: FundComparisonSide,
    pub sector_allocations: Vec<AllocationComparison>,
    pub country_allocations: Vec<AllocationComparison>,
    pub currency_allocations: Vec<AllocationComparison>,
    pub common_holdings: Vec<CommonFundHolding>,
    pub correlation: FundComparisonCorrelation,
    pub risk_free_assumption: RiskFreeAssumption,
}

#[derive(Clone, Copy)]
pub struct FundComparisonPeriod {
    pub label: &'static str,
    pub days: i64,
}

#[derive(Serialize)]
pub struct FundComparisonSide {
    pub code: String,
    pub name: String,
    /// The exact class and vehicle identity of this side.
    pub class_identity: FundClassIdentity,
    pub info: FundInfoComparison,
    pub ytd: Option<FundPeriodMetrics>,
    pub one_year: Option<FundPeriodMetrics>,
    pub three_year: Option<FundPeriodMetrics>,
    pub five_year: Option<FundPeriodMetrics>,
    pub all_time: Option<FundPeriodMetrics>,
    pub primary_benchmark_comparison: FundBenchmarkComparison,
}

/// The exact class identity of one compared fund.
///
/// Two share classes of the same fund are different investments, so the
/// comparison keeps each side's own identity: the source code that selects the
/// exact class, and the tracked identity — the ISIN for a fund — when this
/// portfolio tracks that exact class. A fund that is not tracked has no local
/// identity, and that absence stays visible instead of being guessed.
#[derive(Serialize)]
pub struct FundClassIdentity {
    /// The source code that selects this exact class.
    pub source_code: String,
    /// The tracked user-facing identity of the exact class, when it is tracked.
    pub tracked_identity: Option<String>,
    /// The tracked vehicle type of the exact class, when it is tracked.
    pub vehicle: Option<String>,
    /// Whether this portfolio tracks the exact class.
    pub tracked: bool,
}

#[derive(Serialize)]
pub struct FundInfoComparison {
    pub currency: Option<String>,
    pub aum: Option<f64>,
    pub aum_currency: Option<String>,
    pub inception_date: Option<String>,
    pub total_holdings: Option<i32>,
    pub top_10_weight: Option<f64>,
    pub portfolio_date: Option<String>,
}

#[derive(Serialize)]
pub struct AllocationComparison {
    pub label: String,
    pub weight_a: f64,
    pub weight_b: f64,
}

#[derive(Serialize)]
pub struct CommonFundHolding {
    pub ticker: Option<String>,
    pub name_a: String,
    pub weight_a: f64,
    pub weight_b: f64,
}

#[derive(Serialize)]
pub struct FundComparisonCorrelation {
    pub period_label: String,
    pub correlation: Option<f64>,
    pub reason: Option<String>,
    pub points: Vec<AlignedFundReturnPoint>,
}

#[derive(Serialize)]
pub struct AlignedFundReturnPoint {
    pub date: String,
    pub return_a: f64,
    pub return_b: f64,
}
