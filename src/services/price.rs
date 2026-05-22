use crate::models::{AssetType, StockInfo};

#[async_trait::async_trait]
pub trait PriceFetcher: Send + Sync {
    async fn get_historical_prices(
        &self,
        ticker: &str,
        start: &str,
        end: &str,
        asset_type: &AssetType,
    ) -> anyhow::Result<Vec<(String, f64)>>;

    async fn get_historical_exchange_rates(
        &self,
        pair: &str,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<(String, f64)>>;

    async fn get_stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo>;
}
