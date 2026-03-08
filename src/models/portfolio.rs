use crate::db::entities::{portfolio_asset_history, portfolio_history};
use tabled::Tabled;

use super::AssetPosition;

pub struct PortfolioSnapshot {
    pub date: String,
    pub asset_value: f64,
    pub total_value: f64,
    pub outstanding_shares: f64,
    pub nav: f64,
}

impl From<portfolio_history::Model> for PortfolioSnapshot {
    fn from(m: portfolio_history::Model) -> Self {
        Self {
            date: m.date,
            asset_value: m.asset_value,
            total_value: m.total_value,
            outstanding_shares: m.outstanding_shares,
            nav: m.nav,
        }
    }
}

pub struct AssetSnapshot {
    pub date: String,
    pub asset_id: i32,
    pub quantity: f64,
    pub closing_price: f64,
    pub market_value: f64,
    pub exchange_rate: f64,
}

impl From<portfolio_asset_history::Model> for AssetSnapshot {
    fn from(m: portfolio_asset_history::Model) -> Self {
        Self {
            date: m.date,
            asset_id: m.asset_id,
            quantity: m.quantity,
            closing_price: m.closing_price,
            market_value: m.market_value,
            exchange_rate: m.exchange_rate,
        }
    }
}

pub struct PortfolioResult {
    pub rows: Vec<AssetPosition>,
    pub total_invested: f64,
    pub total_current_value: f64,
    pub total_gain_loss: f64,
    pub total_gain_loss_pct: f64,
}

pub struct PortfolioSummary {
    pub total_value: f64,
    pub nav: f64,
    pub snapshot_date: String,
    pub daily_change: Option<f64>,
    pub daily_change_pct: Option<f64>,
    pub inception_date: Option<String>,
    pub ytd_return: Option<f64>,
    pub one_year_return: Option<f64>,
    pub three_year_return: Option<f64>,
    pub five_year_return: Option<f64>,
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
    #[tabled(rename = "Price Date")]
    pub price_date: String,
    #[tabled(rename = "Invested")]
    pub total_invested: String,
    #[tabled(rename = "Value")]
    pub current_value: String,
    #[tabled(rename = "G/L")]
    pub gain_loss: String,
    #[tabled(rename = "G/L %")]
    pub gain_loss_pct: String,
    #[tabled(rename = "Weight")]
    pub weight: String,
}
