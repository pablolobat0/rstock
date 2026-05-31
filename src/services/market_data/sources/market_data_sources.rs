use std::collections::BTreeMap;

use chrono::NaiveDate;

use crate::models::{FundData, FundQuoteMetadata, StockInfo};

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

    async fn fund_data(&self, code: &str, limit: u32) -> anyhow::Result<FundData>;

    async fn fund_quote_metadata(&self, code: &str) -> anyhow::Result<FundQuoteMetadata>;
}

pub(super) fn sort_and_dedup_observations(
    values: Vec<SourceObservation>,
) -> Vec<SourceObservation> {
    let mut by_date = BTreeMap::new();
    for observation in values {
        by_date.insert(observation.date, observation.value);
    }
    by_date
        .into_iter()
        .map(|(date, value)| SourceObservation { date, value })
        .collect()
}
