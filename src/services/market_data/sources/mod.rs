mod market_data_sources;
mod morningstar;
mod yahoo;

use chrono::NaiveDate;

use crate::models::{FundData, StockInfo};
use crate::settings::Settings;

pub use market_data_sources::{MarketDataSources, SourceObservation};

use morningstar::MorningstarAdapter;
use yahoo::YahooFinanceAdapter;

pub struct DefaultMarketDataSources {
    yahoo: YahooFinanceAdapter,
    morningstar: MorningstarAdapter,
}

impl DefaultMarketDataSources {
    pub fn new() -> anyhow::Result<Self> {
        let settings = Settings::from_env()?;
        Ok(Self {
            yahoo: YahooFinanceAdapter,
            morningstar: MorningstarAdapter::new(settings),
        })
    }
}

#[async_trait::async_trait]
impl MarketDataSources for DefaultMarketDataSources {
    async fn stock_price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.yahoo.price_history(ticker, start, end).await
    }

    async fn fund_price_history(
        &self,
        code: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.morningstar.price_history(code, start, end).await
    }

    async fn exchange_rate_history(
        &self,
        from: &str,
        to: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.yahoo.exchange_rate_history(from, to, start, end).await
    }

    async fn stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo> {
        self.yahoo.stock_info(ticker).await
    }

    async fn fund_data(&self, code: &str, limit: u32) -> anyhow::Result<FundData> {
        self.morningstar.fund_data(code, limit).await
    }
}
