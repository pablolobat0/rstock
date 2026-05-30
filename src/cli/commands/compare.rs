use sea_orm::DatabaseConnection;

use crate::services;
use crate::services::market_data::MarketData;

use super::super::display;
use super::super::CorrelationPeriod;

pub async fn funds(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code_a: String,
    code_b: String,
    _period: CorrelationPeriod,
) -> anyhow::Result<()> {
    let result =
        services::fund_comparison::compare_funds(db, market_data, &code_a, &code_b).await?;
    display::print_fund_comparison(&result);
    Ok(())
}
