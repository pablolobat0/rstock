pub mod sources;

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::DATE_FORMAT;
use crate::db::repos::asset_repo;
use crate::models::{
    Asset, AssetClassification, AssetType, CorrelationMarketData, CorrelationMarketDataSeries,
    FundData, IndividualPrice, IndividualPriceFallback, MarketDataValuation, StockInfo,
    ValuationMarketData,
};
use crate::services::metrics;
use crate::services::price::PriceFetcher;
use crate::services::{historical_market_data, individual_price};

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

    pub async fn fund_data(&self, code: &str, limit: u32) -> anyhow::Result<FundData> {
        self.sources.fund_data(code, limit).await
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

    pub async fn individual_price(
        &self,
        db: &DatabaseConnection,
        asset: &Asset,
        fallback: IndividualPriceFallback,
    ) -> anyhow::Result<IndividualPrice> {
        individual_price::get_individual_price(db, asset, fallback, self).await
    }

    pub async fn correlation_market_data(
        &self,
        db: &DatabaseConnection,
        tracked_assets: Vec<Asset>,
        start_date: &str,
        end_date: &str,
    ) -> anyhow::Result<CorrelationMarketData> {
        let benchmark = get_or_create_benchmark_asset(db).await?;
        let mut all_assets = tracked_assets.clone();
        all_assets.push(benchmark.clone());

        let prepared = self
            .prepare_valuation_market_data(db, &all_assets, start_date, end_date)
            .await?;

        let mut tracked_asset_series = Vec::with_capacity(tracked_assets.len());
        for asset in tracked_assets {
            tracked_asset_series.push(correlation_series(db, &asset, start_date, end_date).await?);
        }

        let benchmark_series = correlation_series(db, &benchmark, start_date, end_date).await?;

        Ok(CorrelationMarketData {
            requested_start_date: start_date.to_owned(),
            requested_end_date: end_date.to_owned(),
            tracked_asset_series,
            benchmark_series,
            limitations: prepared.limitations,
        })
    }
}

async fn get_or_create_benchmark_asset(db: &DatabaseConnection) -> anyhow::Result<Asset> {
    let info = metrics::benchmark_asset_info();
    if let Some(asset) = asset_repo::find_by_ticker(db, &info.ticker).await? {
        return Ok(asset);
    }

    let id = asset_repo::create(db, &info, &AssetClassification::default(), None).await?;
    Ok(metrics::benchmark_asset(id))
}

async fn correlation_series(
    db: &DatabaseConnection,
    asset: &Asset,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<CorrelationMarketDataSeries> {
    let prices: crate::models::BaseCurrencyPriceSeries =
        historical_market_data::get_base_currency_price_series(db, asset, start_date, end_date)
            .await?;

    Ok(CorrelationMarketDataSeries {
        asset_id: asset.id,
        name: asset.name.clone(),
        prices,
    })
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
