#[derive(Clone)]
pub struct StockInfo {
    pub ticker: String,
    pub name: Option<String>,
    pub currency: Option<String>,
    pub current_price: Option<f64>,
    pub previous_close: Option<f64>,
    pub day_range: Option<(f64, f64)>,
    pub fifty_two_week_range: Option<(f64, f64)>,
    pub volume: Option<u64>,
    pub avg_volume: Option<u64>,
    pub market_cap: Option<f64>,
    pub pe_ttm: Option<f64>,
    pub eps_ttm: Option<f64>,
    pub dividend_yield: Option<f64>,
    pub sector: Option<String>,
    pub industry: Option<String>,
    pub country: Option<String>,
}

pub struct MomentumIndicators {
    pub rsi_14: Option<f64>,
    pub sma_50: Option<f64>,
    pub sma_200: Option<f64>,
    pub sma_50_signal: Option<String>,
    pub sma_200_signal: Option<String>,
    pub golden_death_cross: Option<String>,
    pub macd_line: Option<f64>,
    pub macd_signal: Option<f64>,
    pub macd_histogram: Option<f64>,
    pub macd_signal_text: Option<String>,
}

pub struct RelationshipMetrics {
    pub relative_strength_current: Option<f64>,
    pub relative_strength_change: Option<f64>,
    pub beta_vs_sector: Option<f64>,
    pub correlation: Option<f64>,
}

pub struct MonitorReport {
    pub stock_info: StockInfo,
    pub stock_momentum: MomentumIndicators,
    pub sector_etf_ticker: String,
    pub sector_momentum: MomentumIndicators,
    pub relationship: RelationshipMetrics,
    pub stock_prices: Vec<(String, f64)>,
    pub sector_prices: Vec<(String, f64)>,
    pub period_label: String,
}
