use super::FundPeriodMetrics;

pub struct FundComparisonResult {
    pub fund_a: FundComparisonSide,
    pub fund_b: FundComparisonSide,
    pub sector_allocations: Vec<AllocationComparison>,
    pub country_allocations: Vec<AllocationComparison>,
    pub currency_allocations: Vec<AllocationComparison>,
    pub common_holdings: Vec<CommonFundHolding>,
    pub correlation: FundComparisonCorrelation,
}

#[derive(Clone, Copy)]
pub struct FundComparisonPeriod {
    pub label: &'static str,
    pub days: i64,
}

pub struct FundComparisonSide {
    pub code: String,
    pub name: String,
    pub info: FundInfoComparison,
    pub ytd: Option<FundPeriodMetrics>,
    pub one_year: Option<FundPeriodMetrics>,
    pub three_year: Option<FundPeriodMetrics>,
    pub five_year: Option<FundPeriodMetrics>,
    pub all_time: Option<FundPeriodMetrics>,
}

pub struct FundInfoComparison {
    pub currency: Option<String>,
    pub aum: Option<f64>,
    pub aum_currency: Option<String>,
    pub inception_date: Option<String>,
    pub total_holdings: Option<i32>,
    pub top_10_weight: Option<f64>,
    pub portfolio_date: Option<String>,
}

pub struct AllocationComparison {
    pub label: String,
    pub weight_a: f64,
    pub weight_b: f64,
}

pub struct CommonFundHolding {
    pub ticker: Option<String>,
    pub name_a: String,
    pub weight_a: f64,
    pub weight_b: f64,
}

pub struct FundComparisonCorrelation {
    pub period_label: String,
    pub correlation: Option<f64>,
    pub reason: Option<String>,
    pub points: Vec<AlignedFundReturnPoint>,
}

pub struct AlignedFundReturnPoint {
    pub date: String,
    pub return_a: f64,
    pub return_b: f64,
}
