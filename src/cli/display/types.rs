use tabled::Tabled;

#[derive(Tabled)]
pub struct TransactionRow {
    #[tabled(rename = "ID")]
    pub id: i32,
    #[tabled(rename = "Date")]
    pub date: String,
    #[tabled(rename = "Type")]
    pub tx_type: String,
    #[tabled(rename = "Ticker")]
    pub ticker: String,
    #[tabled(rename = "Name")]
    pub asset_name: String,
    #[tabled(rename = "Quantity")]
    pub quantity: String,
    #[tabled(rename = "Price/Amount")]
    pub price: String,
    #[tabled(rename = "Fees")]
    pub fees: String,
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
pub struct TopHoldingRow {
    #[tabled(rename = "Company")]
    pub name: String,
    #[tabled(rename = "Ticker")]
    pub ticker: String,
    #[tabled(rename = "Weight")]
    pub weight: String,
    #[tabled(rename = "Country")]
    pub country: String,
    #[tabled(rename = "Sector")]
    pub sector: String,
}
