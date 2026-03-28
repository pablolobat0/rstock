use std::fmt;
use std::str::FromStr;

use crate::db::entities::transaction;

pub fn f64_to_cents(val: f64) -> i64 {
    (val * 100.0).round() as i64
}

pub fn cents_to_f64(cents: i64) -> f64 {
    cents as f64 / 100.0
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
    pub notes: Option<String>,
}

pub struct SellOrder {
    pub date: String,
    pub quantity: f64,
    pub price: f64,
    pub fees: f64,
    pub notes: Option<String>,
}

pub struct DividendOrder {
    pub date: String,
    pub amount: f64,
    pub fees: f64,
    pub notes: Option<String>,
}

pub struct SplitOrder {
    pub date: String,
    pub ratio: f64,
    pub notes: Option<String>,
}

pub struct Transaction {
    pub asset_id: i32,
    pub tx_type: TxType,
    pub date: String,
    pub quantity: f64,
    pub price_cents: i64,
    pub fees_cents: i64,
}

impl Transaction {
    pub fn is_buy(&self) -> bool {
        self.tx_type == TxType::Buy
    }

    pub fn is_sell(&self) -> bool {
        self.tx_type == TxType::Sell
    }

    pub fn is_dividend(&self) -> bool {
        self.tx_type == TxType::Dividend
    }

    pub fn is_split(&self) -> bool {
        self.tx_type == TxType::Split
    }

    pub fn signed_quantity(&self) -> f64 {
        match self.tx_type {
            TxType::Buy => self.quantity,
            TxType::Sell => -self.quantity,
            TxType::Dividend | TxType::Split => 0.0,
        }
    }

    /// Compute net holdings from a chronologically-ordered slice of transactions,
    /// accounting for splits (which multiply holdings by their ratio).
    pub fn compute_holdings(transactions: &[Transaction]) -> f64 {
        let mut holdings = 0.0;
        for tx in transactions {
            if tx.is_split() {
                holdings *= tx.quantity;
            } else {
                holdings += tx.signed_quantity();
            }
        }
        holdings
    }
}

impl From<transaction::Model> for Transaction {
    fn from(m: transaction::Model) -> Self {
        Self {
            asset_id: m.asset_id,
            tx_type: m.tx_type.parse().expect("invalid tx_type in DB"),
            date: m.date,
            quantity: m.quantity,
            price_cents: m.price_cents,
            fees_cents: m.fees_cents,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cents_roundtrip() {
        assert_eq!(f64_to_cents(150.25), 15025);
        assert_eq!(cents_to_f64(15025), 150.25);
    }

    #[test]
    fn test_f64_to_cents_rounds() {
        assert_eq!(f64_to_cents(1.006), 101);
        assert_eq!(f64_to_cents(0.0), 0);
        assert_eq!(f64_to_cents(99.999), 10000);
    }

    #[test]
    fn test_cents_to_f64_negative() {
        assert_eq!(cents_to_f64(-500), -5.0);
    }
}
