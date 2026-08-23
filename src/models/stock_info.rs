#[derive(Clone)]
pub struct StockInfo {
    pub name: Option<String>,
    pub market_cap: Option<f64>,
    pub sector: Option<String>,
    pub country: Option<String>,
}
