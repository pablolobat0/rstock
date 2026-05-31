mod historical;
mod individual_price;
mod policy;
pub mod sources;

use anyhow::bail;
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::db::repos::asset_repo;
use crate::models::{
    Asset, AssetClassification, CorrelationMarketData, CorrelationMarketDataSeries, FundData,
    FundQuoteMetadata, IndividualPrice, IndividualPriceFallback, MarketDataValuation, StockInfo,
    ValuationMarketData,
};
use crate::services::metrics;

pub use sources::{DefaultMarketDataSources, MarketDataSources, SourceObservation};

pub struct MarketData {
    sources: Box<dyn MarketDataSources>,
}

impl MarketData {
    pub fn new(sources: Box<dyn MarketDataSources>) -> Self {
        Self { sources }
    }

    pub(crate) async fn stock_price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.sources.stock_price_history(ticker, start, end).await
    }

    pub(crate) async fn fund_price_history(
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

    pub async fn fund_quote_metadata(&self, code: &str) -> anyhow::Result<FundQuoteMetadata> {
        self.sources.fund_quote_metadata(code).await
    }

    pub async fn prepare_valuation_market_data(
        &self,
        db: &DatabaseConnection,
        assets: &[Asset],
        start_date: &str,
        end_date: &str,
    ) -> anyhow::Result<ValuationMarketData> {
        historical::prepare_valuation_market_data(db, assets, start_date, end_date, self).await
    }

    pub async fn get_required_asset_exchange_rates(
        &self,
        db: &DatabaseConnection,
        assets: &[Asset],
        date: &str,
    ) -> anyhow::Result<std::collections::HashMap<i32, f64>> {
        historical::get_required_asset_exchange_rates(db, assets, date).await
    }

    pub async fn get_required_asset_valuation_data(
        &self,
        db: &DatabaseConnection,
        asset: &Asset,
        date: &str,
    ) -> anyhow::Result<MarketDataValuation> {
        historical::get_required_asset_valuation_data(db, asset, date).await
    }

    pub async fn get_asset_exchange_rate(
        &self,
        db: &DatabaseConnection,
        asset: &Asset,
        date: &str,
    ) -> anyhow::Result<Option<f64>> {
        historical::get_exchange_rate_for_asset(db, asset, date).await
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

    pub async fn tracked_correlation_market_data(
        &self,
        db: &DatabaseConnection,
        tracked_assets: Vec<Asset>,
        start_date: &str,
        end_date: &str,
    ) -> anyhow::Result<(
        Vec<CorrelationMarketDataSeries>,
        Vec<crate::models::MarketDataLimitation>,
    )> {
        let prepared = self
            .prepare_valuation_market_data(db, &tracked_assets, start_date, end_date)
            .await?;

        let mut tracked_asset_series = Vec::with_capacity(tracked_assets.len());
        for asset in tracked_assets {
            tracked_asset_series.push(correlation_series(db, &asset, start_date, end_date).await?);
        }

        Ok((tracked_asset_series, prepared.limitations))
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
        historical::get_base_currency_price_series(db, asset, start_date, end_date).await?;

    Ok(CorrelationMarketDataSeries {
        asset_id: asset.id,
        name: asset.name.clone(),
        prices,
    })
}

fn normalize_currency(currency: &str) -> anyhow::Result<String> {
    let normalized = currency.trim().to_ascii_uppercase();
    if normalized.len() != 3 || !normalized.chars().all(|ch| ch.is_ascii_alphabetic()) {
        bail!("currency must be a three-letter alphabetic code: {currency}");
    }
    Ok(normalized)
}
