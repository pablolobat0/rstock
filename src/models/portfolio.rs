use crate::db::entities::{portfolio_asset_history, portfolio_history};
use serde::Serialize;

use super::{AssetPosition, CurrentPosition, MarketDataLimitation, MonetaryPosition};

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

#[derive(Serialize)]
pub struct PeriodMetrics {
    pub volatility: Option<f64>,
    pub max_drawdown: Option<f64>,
    pub beta: Option<f64>,
    pub sharpe: Option<f64>,
    pub sortino: Option<f64>,
}

#[derive(Serialize)]
pub struct PortfolioResult {
    pub base_currency: String,
    #[serde(rename = "positions")]
    pub rows: Vec<AssetPosition>,
    pub monetary_positions: Vec<MonetaryPosition>,
    pub total_monetary_value: Option<f64>,
    pub total_invested: f64,
    pub total_current_value: f64,
    pub total_dividends: f64,
    #[serde(rename = "total_open_position_gain_loss")]
    pub total_gain_loss: f64,
    #[serde(rename = "total_open_position_gain_loss_pct")]
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
    pub market_data_limitations: Vec<MarketDataLimitation>,
    pub monetary_market_data_limitations: Vec<MarketDataLimitation>,
}

#[derive(Serialize)]
#[allow(dead_code)] // Used through the library portfolio interface while CLI callers migrate.
pub struct CurrentPositions {
    pub base_currency: String,
    pub positions: Vec<CurrentPosition>,
    pub monetary_positions: Vec<CurrentPosition>,
    pub total_current_value: Option<f64>,
    pub total_monetary_value: Option<f64>,
    pub total_value: Option<f64>,
    pub total_invested: Option<f64>,
    pub total_dividends: Option<f64>,
    #[serde(rename = "total_open_position_gain_loss")]
    pub total_gain_loss: Option<f64>,
    #[serde(rename = "total_open_position_gain_loss_pct")]
    pub total_gain_loss_pct: Option<f64>,
    pub market_data_limitations: Vec<MarketDataLimitation>,
    pub monetary_market_data_limitations: Vec<MarketDataLimitation>,
}

#[derive(Serialize)]
pub struct CorrelationMatrix {
    /// Display names in order (assets + reference index)
    pub names: Vec<String>,
    /// N×N matrix of Option<f64>; None = insufficient data
    pub matrix: Vec<Vec<Option<f64>>>,
    /// Names with insufficient data for the requested period
    pub warnings: Vec<String>,
    pub market_data_limitations: Vec<MarketDataLimitation>,
}

#[derive(Serialize)]
pub struct RollingCorrelationResult {
    pub left_name: String,
    pub right_name: String,
    pub period_label: String,
    pub window_label: String,
    pub requested_start_date: String,
    pub requested_end_date: String,
    pub points: Vec<(String, f64)>,
    pub latest: Option<f64>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub average: Option<f64>,
    pub market_data_limitations: Vec<MarketDataLimitation>,
}

// --- Holdings report models ---

#[derive(Clone, Serialize)]
pub struct FundHolding {
    pub name: String,
    /// Weight within the fund (0–100 percentage)
    pub weighting: f64,
    pub ticker: Option<String>,
    pub sector: Option<String>,
    pub country: Option<String>,
    pub currency: Option<String>,
}

// --- Composition analysis models ---

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum MarketCapCategory {
    Large,
    Mid,
    Small,
}

impl std::fmt::Display for MarketCapCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarketCapCategory::Large => write!(f, "Large Cap"),
            MarketCapCategory::Mid => write!(f, "Mid Cap"),
            MarketCapCategory::Small => write!(f, "Small Cap"),
        }
    }
}

#[derive(serde::Serialize)]
pub struct AllocationEntry {
    pub label: String,
    /// Weight as a percentage (0–100)
    pub weight: f64,
}

#[derive(serde::Serialize)]
pub struct TopHolding {
    pub name: String,
    pub ticker: Option<String>,
    /// Weight as a percentage (0–100) of the equity portfolio
    pub weight: f64,
    pub country: Option<String>,
    pub sector: Option<String>,
}

#[derive(serde::Serialize)]
pub struct CompositionResult {
    pub asset_class_breakdown: Vec<AllocationEntry>,
    pub equity_style_breakdown: Vec<AllocationEntry>,
    pub management_breakdown: Vec<AllocationEntry>,
    pub sector_breakdown: Vec<AllocationEntry>,
    pub country_breakdown: Vec<AllocationEntry>,
    pub market_cap_breakdown: Vec<AllocationEntry>,
    pub top_holdings: Vec<TopHolding>,
    pub warnings: Vec<String>,
}
