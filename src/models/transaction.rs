use crate::db::entities::transaction;

pub fn f64_to_cents(val: f64) -> i64 {
    (val * 100.0).round() as i64
}

pub fn cents_to_f64(cents: i64) -> f64 {
    cents as f64 / 100.0
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

pub struct Transaction {
    pub asset_id: i32,
    pub tx_type: String,
    pub date: String,
    pub quantity: f64,
    pub price_cents: i64,
    pub fees_cents: i64,
}

impl From<transaction::Model> for Transaction {
    fn from(m: transaction::Model) -> Self {
        Self {
            asset_id: m.asset_id,
            tx_type: m.tx_type,
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
