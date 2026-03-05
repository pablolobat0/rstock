use crate::db::entities::transaction;

pub struct BuyOrder {
    pub date: String,
    pub quantity: f64,
    pub price: f64,
    pub fees: f64,
    pub notes: Option<String>,
}

pub struct Transaction {
    pub asset_id: i32,
    pub date: String,
    pub quantity: f64,
    pub price_cents: i64,
    pub fees_cents: i64,
}

impl From<transaction::Model> for Transaction {
    fn from(m: transaction::Model) -> Self {
        Self {
            asset_id: m.asset_id,
            date: m.date,
            quantity: m.quantity,
            price_cents: m.price_cents,
            fees_cents: m.fees_cents,
        }
    }
}
