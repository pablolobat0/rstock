use crate::db::entities::{portfolio_asset_history, portfolio_history};

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

pub struct PeriodMetrics {
    pub volatility: Option<f64>,
    pub max_drawdown: Option<f64>,
    pub beta: Option<f64>,
    pub sharpe: Option<f64>,
}

pub struct PortfolioResult {
    pub rows: Vec<AssetPosition>,
    pub total_invested: f64,
    pub total_current_value: f64,
    pub total_dividends: f64,
    pub total_gain_loss: f64,
    pub total_gain_loss_pct: f64,
    pub snapshot_date: Option<String>,
    pub nav: Option<f64>,
    pub daily_change: Option<f64>,
    pub daily_change_pct: Option<f64>,
    pub inception_date: Option<String>,
    pub ytd_return: Option<f64>,
    pub one_year_return: Option<f64>,
    pub three_year_return: Option<f64>,
    pub five_year_return: Option<f64>,
    pub ytd_metrics: Option<PeriodMetrics>,
    pub one_year_metrics: Option<PeriodMetrics>,
    pub three_year_metrics: Option<PeriodMetrics>,
    pub five_year_metrics: Option<PeriodMetrics>,
}

pub struct CorrelationMatrix {
    /// Display names in order (assets + reference index)
    pub names: Vec<String>,
    /// N×N matrix of Option<f64>; None = insufficient data
    pub matrix: Vec<Vec<Option<f64>>>,
    /// Names with insufficient data for the requested period
    pub warnings: Vec<String>,
}

// --- Holdings report models ---

pub struct FundHolding {
    pub name: String,
    /// Weight within the fund (0–100 percentage)
    pub weighting: f64,
    pub ticker: Option<String>,
    pub sector: Option<String>,
}

pub struct DirectHolding {
    pub ticker: String,
    pub name: String,
    /// Weight in the total portfolio (0–100 percentage)
    pub portfolio_weight: f64,
    pub current_value: f64,
}

pub struct FundWithHoldings {
    pub ticker: String,
    pub name: String,
    /// Weight of this fund in the total portfolio (0–100 percentage)
    pub portfolio_weight: f64,
    pub current_value: f64,
    pub holdings: Vec<FundHolding>,
    pub error: Option<String>,
}

pub struct HoldingsResult {
    pub stocks: Vec<DirectHolding>,
    pub funds: Vec<FundWithHoldings>,
    pub total_portfolio_value: f64,
}
