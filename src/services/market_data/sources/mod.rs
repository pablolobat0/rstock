use chrono::NaiveDate;

use crate::models::StockInfo;

mod production;

pub use production::DefaultMarketDataSources;

#[derive(Clone, Debug, PartialEq)]
pub struct SourceObservation {
    pub date: NaiveDate,
    pub value: f64,
}

#[async_trait::async_trait]
pub trait MarketDataSources: Send + Sync {
    async fn stock_price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>>;

    async fn fund_price_history(
        &self,
        code: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>>;

    async fn exchange_rate_history(
        &self,
        from: &str,
        to: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>>;

    async fn stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo>;
}
