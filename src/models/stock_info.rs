#[derive(Clone)]
#[allow(dead_code)]
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
