use tabled::Tabled;

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
}

#[derive(Tabled)]
pub struct PortfolioRow {
    #[tabled(rename = "Ticker")]
    pub ticker: String,
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Type")]
    pub asset_type: String,
    #[tabled(rename = "Currency")]
    pub currency: String,
    #[tabled(rename = "Quantity")]
    pub quantity: String,
    #[tabled(rename = "Avg Cost")]
    pub avg_cost: String,
    #[tabled(rename = "Price")]
    pub current_price: String,
    #[tabled(rename = "Price Date")]
    pub price_date: String,
    #[tabled(rename = "Invested")]
    pub total_invested: String,
    #[tabled(rename = "Value")]
    pub current_value: String,
    #[tabled(rename = "Divs")]
    pub dividends: String,
    #[tabled(rename = "G/L")]
    pub gain_loss: String,
    #[tabled(rename = "G/L %")]
    pub gain_loss_pct: String,
    #[tabled(rename = "Weight")]
    pub weight: String,
}

#[derive(Tabled)]
pub struct DirectHoldingRow {
    #[tabled(rename = "Ticker")]
    pub ticker: String,
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Value")]
    pub current_value: String,
    #[tabled(rename = "Weight")]
    pub portfolio_weight: String,
}

#[derive(Tabled)]
pub struct FundHoldingRow {
    #[tabled(rename = "Name")]
    pub name: String,
    #[tabled(rename = "Fund Weight")]
    pub fund_weight: String,
    #[tabled(rename = "Portfolio Weight")]
    pub effective_weight: String,
}
