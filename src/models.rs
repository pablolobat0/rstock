use serde::Deserialize;
use tabled::Tabled;

pub struct AssetInfo {
    pub ticker: String,
    pub name: String,
    pub asset_type: String,
    pub isin: Option<String>,
    pub currency: String,
}

pub struct BuyOrder {
    pub date: String,
    pub quantity: f64,
    pub price: f64,
    pub fees: f64,
    pub notes: Option<String>,
}

#[derive(Tabled)]
pub struct PortfolioRow {
    #[tabled(rename = "Ticker")]
    pub ticker: String,
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Type")]
    pub asset_type: String,
    #[tabled(rename = "Currency")]
    pub currency: String,
    #[tabled(rename = "Quantity")]
    pub quantity: String,
    #[tabled(rename = "Avg Cost")]
    pub avg_cost: String,
    #[tabled(rename = "Price")]
    pub current_price: String,
    #[tabled(rename = "Invested")]
    pub total_invested: String,
    #[tabled(rename = "Value")]
    pub current_value: String,
    #[tabled(rename = "G/L")]
    pub gain_loss: String,
    #[tabled(rename = "G/L %")]
    pub gain_loss_pct: String,
}

#[derive(Deserialize)]
pub struct FundPriceResponse {
    pub price: f64,
    #[allow(dead_code)]
    pub date: String,
}
