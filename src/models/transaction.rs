use std::fmt;
use std::str::FromStr;

use crate::constants::MONETARY_MULTIPLIER;
use crate::db::entities::transaction;

pub fn f64_to_cents(val: f64) -> i64 {
    (val * MONETARY_MULTIPLIER).round() as i64
}

pub fn cents_to_f64(cents: i64) -> f64 {
    cents as f64 / MONETARY_MULTIPLIER
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxType {
    Buy,
    Sell,
    Dividend,
    Split,
}

impl fmt::Display for TxType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TxType::Buy => write!(f, "buy"),
            TxType::Sell => write!(f, "sell"),
            TxType::Dividend => write!(f, "dividend"),
            TxType::Split => write!(f, "split"),
        }
    }
}

impl FromStr for TxType {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "buy" => Ok(TxType::Buy),
            "sell" => Ok(TxType::Sell),
            "dividend" => Ok(TxType::Dividend),
            "split" => Ok(TxType::Split),
            other => anyhow::bail!("unknown transaction type: {other}"),
        }
    }
}

pub struct BuyOrder {
    pub date: String,
    pub quantity: f64,
    pub price: f64,
    pub fees: f64,
}

pub struct SellOrder {
    pub date: String,
    pub quantity: f64,
    pub price: f64,
    pub fees: f64,
}

pub struct DividendOrder {
    pub date: String,
    pub amount: f64,
    pub fees: f64,
}

pub struct SplitOrder {
    pub date: String,
    pub ratio: f64,
}

pub struct CsvRow {
    pub source_row: usize,
    pub date: chrono::NaiveDate,
    pub ticker: String,
    pub name: Option<String>,
    pub asset_type: Option<super::AssetType>,
    pub currency: Option<String>,
    pub morningstar_code: Option<String>,
    pub classification: super::AssetClassification,
    pub tx_type: TxType,
    pub quantity: f64,
    pub price: f64,
    pub fees: f64,
}

#[derive(Clone)]
pub struct Transaction {
    pub id: i32,
    pub asset_id: i32,
    pub tx_type: TxType,
    pub date: String,
    /// Semantic persistence fields.  Only the fields meaningful to `tx_type`
    /// are populated; the database enforces the same shape.
    pub units: Option<f64>,
    pub unit_price_cents: Option<i64>,
    pub dividend_amount_cents: Option<i64>,
    pub dividend_deductions_cents: Option<i64>,
    pub split_ratio: Option<f64>,
    pub trade_fees_cents: Option<i64>,
    // Compatibility projections for older clients. New domain code uses the
    // semantic fields above.
    #[allow(dead_code)]
    pub quantity: f64,
    #[allow(dead_code)]
    pub price_cents: i64,
    #[allow(dead_code)]
    pub fees_cents: i64,
}

pub struct TransactionListItem {
    pub transaction: Transaction,
    pub ticker: String,
    pub asset_name: String,
}

impl Transaction {
    #[must_use]
    pub fn display_quantity(&self) -> f64 {
        match &self.tx_type {
            TxType::Buy | TxType::Sell => self.units.unwrap_or_default(),
            TxType::Dividend => 1.0,
            TxType::Split => self.split_ratio.unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn display_price_cents(&self) -> i64 {
        match &self.tx_type {
            TxType::Buy | TxType::Sell => self.unit_price_cents.unwrap_or_default(),
            TxType::Dividend => self.dividend_amount_cents.unwrap_or_default(),
            TxType::Split => 0,
        }
    }

    #[must_use]
    pub fn display_fees_cents(&self) -> i64 {
        match &self.tx_type {
            TxType::Buy | TxType::Sell => self.trade_fees_cents.unwrap_or_default(),
            TxType::Dividend => self.dividend_deductions_cents.unwrap_or_default(),
            TxType::Split => 0,
        }
    }

    #[must_use]
    pub fn ledger_units(&self) -> Option<f64> {
        self.units
    }

    #[must_use]
    pub fn ledger_unit_price_cents(&self) -> Option<i64> {
        self.unit_price_cents
    }

    #[must_use]
    pub fn ledger_fees_cents(&self) -> Option<i64> {
        self.trade_fees_cents
    }

    #[must_use]
    pub fn ledger_dividend_amount_cents(&self) -> Option<i64> {
        self.dividend_amount_cents
    }

    #[must_use]
    pub fn ledger_dividend_deductions_cents(&self) -> Option<i64> {
        self.dividend_deductions_cents
    }

    #[must_use]
    pub fn ledger_split_ratio(&self) -> Option<f64> {
        self.split_ratio
    }

    #[allow(dead_code)]
    pub fn is_buy(&self) -> bool {
        self.tx_type == TxType::Buy
    }

    #[allow(dead_code)]
    pub fn is_sell(&self) -> bool {
        self.tx_type == TxType::Sell
    }

    #[allow(dead_code)]
    pub fn is_dividend(&self) -> bool {
        self.tx_type == TxType::Dividend
    }

    pub fn is_split(&self) -> bool {
        self.tx_type == TxType::Split
    }
}

impl From<transaction::Model> for Transaction {
    fn from(m: transaction::Model) -> Self {
        let tx_type = m.tx_type.parse().expect("invalid tx_type in DB");
        let quantity = match &tx_type {
            TxType::Buy | TxType::Sell => m.units.unwrap_or_default(),
            TxType::Dividend => 1.0,
            TxType::Split => m.split_ratio.unwrap_or_default(),
        };
        let price_cents = match &tx_type {
            TxType::Buy | TxType::Sell => m.unit_price_cents.unwrap_or_default(),
            TxType::Dividend => m.dividend_amount_cents.unwrap_or_default(),
            TxType::Split => 0,
        };
        let fees_cents = match &tx_type {
            TxType::Buy | TxType::Sell => m.fees_cents.unwrap_or_default(),
            TxType::Dividend => m.dividend_deductions_cents.unwrap_or_default(),
            TxType::Split => 0,
        };
        Self {
            id: m.id,
            asset_id: m.asset_id,
            tx_type,
            date: m.date,
            units: m.units,
            unit_price_cents: m.unit_price_cents,
            dividend_amount_cents: m.dividend_amount_cents,
            dividend_deductions_cents: m.dividend_deductions_cents,
            split_ratio: m.split_ratio,
            trade_fees_cents: m.fees_cents,
            quantity,
            price_cents,
            fees_cents,
        }
    }
}
