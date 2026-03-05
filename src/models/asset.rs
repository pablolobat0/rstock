use crate::db::entities::asset;

pub struct AssetInfo {
    pub ticker: String,
    pub name: String,
    pub asset_type: String,
    pub isin: Option<String>,
    pub currency: String,
}

pub struct Asset {
    pub id: i32,
    pub ticker: String,
    pub isin: Option<String>,
    pub name: String,
    pub asset_type: String,
    pub currency: String,
}

impl From<asset::Model> for Asset {
    fn from(m: asset::Model) -> Self {
        Self {
            id: m.id,
            ticker: m.ticker,
            isin: m.isin,
            name: m.name,
            asset_type: m.asset_type,
            currency: m.currency,
        }
    }
}

pub struct AssetPosition {
    pub ticker: String,
    pub name: String,
    pub asset_type: String,
    pub currency: String,
    pub total_qty: f64,
    pub avg_cost: f64,
    pub current_price: f64,
    pub total_invested: f64,
    pub current_value: f64,
    pub gain_loss: f64,
    pub gain_loss_pct: f64,
}
