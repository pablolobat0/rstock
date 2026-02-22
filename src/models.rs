pub struct AssetInfo {
    pub ticker: String,
    pub name: String,
    pub asset_type: String,
    pub isin: Option<String>,
    pub currency: String,
}

pub struct BuyOrder {
    pub date: String,
    pub quantity: f64,
    pub price: f64,
    pub fees: f64,
    pub notes: Option<String>,
}
