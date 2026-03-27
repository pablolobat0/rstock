use std::fmt;
use std::str::FromStr;

use clap::ValueEnum;
use tabled::Tabled;

use crate::db::entities::asset;

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq)]
pub enum AssetType {
    Stock,
    Fund,
    Etf,
}

impl fmt::Display for AssetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssetType::Stock => write!(f, "stock"),
            AssetType::Fund => write!(f, "fund"),
            AssetType::Etf => write!(f, "etf"),
        }
    }
}

impl FromStr for AssetType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "stock" => Ok(AssetType::Stock),
            "fund" => Ok(AssetType::Fund),
            "etf" => Ok(AssetType::Etf),
            other => anyhow::bail!("unknown asset type: {other}"),
        }
    }
}

pub struct AssetInfo {
    pub ticker: String,
    pub name: String,
    pub asset_type: AssetType,
    pub isin: Option<String>,
    pub currency: String,
}

pub struct Asset {
    pub id: i32,
    pub ticker: String,
    pub isin: Option<String>,
    pub name: String,
    pub asset_type: AssetType,
    pub currency: String,
}

impl From<asset::Model> for Asset {
    fn from(m: asset::Model) -> Self {
        Self {
            id: m.id,
            ticker: m.ticker,
            isin: m.isin,
            name: m.name,
            asset_type: m.asset_type.parse().unwrap_or(AssetType::Stock),
            currency: m.currency,
        }
    }
}

#[derive(Tabled)]
pub struct AssetRow {
    #[tabled(rename = "Ticker")]
    pub ticker: String,
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Type")]
    pub asset_type: String,
    #[tabled(rename = "Currency")]
    pub currency: String,
    #[tabled(rename = "ISIN")]
    pub isin: String,
}

pub struct AssetPosition {
    pub ticker: String,
    pub name: String,
    pub asset_type: AssetType,
    pub currency: String,
    pub total_qty: f64,
    pub avg_cost: f64,
    pub current_price: f64,
    pub price_date: String,
    pub total_invested: f64,
    pub current_value: f64,
    pub gain_loss: f64,
    pub gain_loss_pct: f64,
}
