pub mod sources;

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::DATE_FORMAT;
use crate::models::{Asset, AssetType, MarketDataValuation, StockInfo, ValuationMarketData};
use crate::services::historical_market_data;
use crate::services::price::PriceFetcher;

pub use sources::{DefaultMarketDataSources, MarketDataSources, SourceObservation};

pub struct MarketData {
    sources: Box<dyn MarketDataSources>,
}

impl MarketData {
    pub fn new(sources: Box<dyn MarketDataSources>) -> Self {
        Self { sources }
    }

    pub async fn stock_price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.sources.stock_price_history(ticker, start, end).await
    }

    pub async fn fund_price_history(
        &self,
        code: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.sources.fund_price_history(code, start, end).await
    }

    pub async fn exchange_rate_history(
        &self,
        from: &str,
        to: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        let from = normalize_currency(from)?;
        let to = normalize_currency(to)?;

        if from == to {
            return Ok(vec![SourceObservation {
                date: start,
                value: 1.0,
            }]);
        }

        self.sources
            .exchange_rate_history(&from, &to, start, end)
            .await
    }

    pub async fn stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo> {
        self.sources.stock_info(ticker).await
    }

    pub async fn prepare_valuation_market_data(
        &self,
        db: &DatabaseConnection,
        assets: &[Asset],
        start_date: &str,
        end_date: &str,
    ) -> anyhow::Result<ValuationMarketData> {
        historical_market_data::prepare_valuation_market_data(
            db, assets, start_date, end_date, self,
        )
        .await
    }

    pub async fn get_required_asset_exchange_rates(
        &self,
        db: &DatabaseConnection,
        assets: &[Asset],
        date: &str,
    ) -> anyhow::Result<std::collections::HashMap<i32, f64>> {
        historical_market_data::get_required_asset_exchange_rates(db, assets, date).await
    }

    pub async fn get_required_asset_valuation_data(
        &self,
        db: &DatabaseConnection,
        asset: &Asset,
        date: &str,
    ) -> anyhow::Result<MarketDataValuation> {
        historical_market_data::get_required_asset_valuation_data(db, asset, date).await
    }
}

#[async_trait::async_trait]
impl PriceFetcher for MarketData {
    async fn get_historical_prices(
        &self,
        ticker: &str,
        start: &str,
        end: &str,
        asset_type: &AssetType,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        let start_date = parse_date(start, "invalid start date")?;
        let end_date = parse_date(end, "invalid end date")?;
        let observations = match asset_type {
            AssetType::Stock => {
                self.stock_price_history(ticker, start_date, end_date)
                    .await?
            }
            AssetType::Fund | AssetType::Etf => {
                self.fund_price_history(ticker, start_date, end_date)
                    .await?
            }
        };
        Ok(format_observations(observations))
    }

    async fn get_historical_exchange_rates(
        &self,
        pair: &str,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        let (from, to) = split_legacy_pair(pair)?;
        let start_date = parse_date(start, "invalid start date")?;
        let end_date = parse_date(end, "invalid end date")?;
        let observations = self
            .exchange_rate_history(from, to, start_date, end_date)
            .await?;
        Ok(format_observations(observations))
    }

    async fn get_stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo> {
        self.stock_info(ticker).await
    }
}

fn parse_date(value: &str, context: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(value, DATE_FORMAT).context(context.to_owned())
}

fn split_legacy_pair(pair: &str) -> anyhow::Result<(&str, &str)> {
    if pair.len() != 6 {
        bail!("FX pair must contain two three-letter currencies: {pair}");
    }
    Ok((&pair[..3], &pair[3..]))
}

fn normalize_currency(currency: &str) -> anyhow::Result<String> {
    let normalized = currency.trim().to_ascii_uppercase();
    if normalized.len() != 3 || !normalized.chars().all(|ch| ch.is_ascii_alphabetic()) {
        bail!("currency must be a three-letter alphabetic code: {currency}");
    }
    Ok(normalized)
}

fn format_observations(observations: Vec<SourceObservation>) -> Vec<(String, f64)> {
    observations
        .into_iter()
        .map(|observation| {
            (
                observation.date.format(DATE_FORMAT).to_string(),
                observation.value,
            )
        })
        .collect()
}
