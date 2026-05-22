use super::portfolio::{AllocationEntry, FundHolding};

pub struct FundAnalysisResult {
    pub ms_code: String,
    pub name: Option<String>,
    pub fund_currency: Option<String>,
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
    pub holdings_changed: bool,
    pub last_snapshot_date: Option<String>,
    pub holding_diff: Vec<HoldingChange>,
}

pub struct FundPeriodMetrics {
    pub total_return: f64,
    pub cagr: Option<f64>,
    pub volatility: Option<f64>,
    pub sharpe: Option<f64>,
    pub sortino: Option<f64>,
    pub max_drawdown: Option<f64>,
    pub beta: Option<f64>,
}

pub struct HoldingChange {
    pub name: String,
    pub change_type: HoldingChangeType,
    pub old_weight: Option<f64>,
    pub new_weight: Option<f64>,
}

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
