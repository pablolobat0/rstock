use anyhow::Context;
use chrono::NaiveDate;

use crate::constants::DATE_FORMAT;
use crate::models::{AssetType, StockInfo};
use crate::services::price::{PriceFetcher, RealPriceFetcher};

use super::{MarketDataSources, SourceObservation};

pub struct DefaultMarketDataSources {
    yahoo: YahooFinanceAdapter,
    morningstar: MorningstarAdapter,
}

impl DefaultMarketDataSources {
    pub fn new() -> Self {
        Self {
            yahoo: YahooFinanceAdapter,
            morningstar: MorningstarAdapter,
        }
    }
}

impl Default for DefaultMarketDataSources {
    fn default() -> Self {
        Self::new()
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
}

struct YahooFinanceAdapter;

impl YahooFinanceAdapter {
    async fn price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        fetch_price_history(ticker, start, end, &AssetType::Stock).await
    }

    async fn exchange_rate_history(
        &self,
        from: &str,
        to: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        let pair = format!("{from}{to}");
        let values = RealPriceFetcher
            .get_historical_exchange_rates(
                &pair,
                &start.format(DATE_FORMAT).to_string(),
                &end.format(DATE_FORMAT).to_string(),
            )
            .await?;
        parse_observations(values)
    }

    async fn stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo> {
        RealPriceFetcher.get_stock_info(ticker).await
    }
}

struct MorningstarAdapter;

impl MorningstarAdapter {
    async fn price_history(
        &self,
        code: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        fetch_price_history(code, start, end, &AssetType::Fund).await
    }
}

async fn fetch_price_history(
    identifier: &str,
    start: NaiveDate,
    end: NaiveDate,
    asset_type: &AssetType,
) -> anyhow::Result<Vec<SourceObservation>> {
    let values = RealPriceFetcher
        .get_historical_prices(
            identifier,
            &start.format(DATE_FORMAT).to_string(),
            &end.format(DATE_FORMAT).to_string(),
            asset_type,
        )
        .await?;
    parse_observations(values)
}

fn parse_observations(values: Vec<(String, f64)>) -> anyhow::Result<Vec<SourceObservation>> {
    values
        .into_iter()
        .map(|(date, value)| {
            let date = NaiveDate::parse_from_str(&date, DATE_FORMAT)
                .with_context(|| format!("invalid source observation date: {date}"))?;
            Ok(SourceObservation { date, value })
        })
        .collect()
}
