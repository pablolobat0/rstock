use std::fmt;
use std::str::FromStr;

use clap::ValueEnum;
use serde::Serialize;

use crate::db::entities::asset;

use super::MarketDataLimitation;

#[derive(ValueEnum, Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
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
    pub currency: String,
}

#[allow(clippy::struct_field_names)]
#[derive(Clone)]
pub struct Asset {
    pub id: i32,
    pub ticker: String,
    pub name: String,
    pub asset_type: AssetType,
    pub currency: String,
    pub morningstar_code: Option<String>,
    pub asset_class: Option<String>,
    pub equity_style: Option<String>,
    pub bond_credit: Option<String>,
    pub bond_duration: Option<String>,
    pub management: Option<String>,
}

impl From<asset::Model> for Asset {
    fn from(m: asset::Model) -> Self {
        Self {
            id: m.id,
            ticker: m.ticker,
            name: m.name,
            asset_type: m.asset_type.parse().expect("invalid asset_type in DB"),
            currency: m.currency,
            morningstar_code: m.morningstar_code,
            asset_class: m.asset_class,
            equity_style: m.equity_style,
            bond_credit: m.bond_credit,
            bond_duration: m.bond_duration,
            management: m.management,
        }
    }
}

impl Asset {
    pub fn is_monetary(&self) -> bool {
        self.asset_class
            .as_deref()
            .is_some_and(|asset_class| asset_class.eq_ignore_ascii_case("monetary"))
    }
}

#[derive(Serialize)]
#[allow(dead_code)] // Used through the library portfolio interface while CLI callers migrate.
pub struct CurrentPosition {
    pub ticker: String,
    pub name: String,
    pub asset_type: AssetType,
    pub currency: String,
    pub morningstar_code: Option<String>,
    pub asset_class: Option<String>,
    pub equity_style: Option<String>,
    pub management: Option<String>,
    pub total_qty: f64,
    pub avg_cost: Option<f64>,
    pub current_price: Option<f64>,
    pub price_date: Option<String>,
    pub total_invested: Option<f64>,
    pub current_value: Option<f64>,
    pub dividends_received: Option<f64>,
    pub open_position_gain_loss: Option<f64>,
    pub open_position_gain_loss_pct: Option<f64>,
    pub market_data_limitations: Vec<MarketDataLimitation>,
}

/// Compatibility aliases retain the two portfolio-view collections while both
/// use identical availability-aware position semantics.
pub type AssetPosition = CurrentPosition;
pub type MonetaryPosition = CurrentPosition;
