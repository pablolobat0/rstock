mod historical;
mod individual_price;
mod policy;
pub mod sources;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use futures::future::{BoxFuture, FutureExt, Shared};
use sea_orm::DatabaseConnection;
use tokio::sync::Semaphore;

use crate::db::repos::asset_repo;
use crate::models::{
    Asset, AssetClassification, CorrelationMarketData, CorrelationMarketDataSeries, FundData,
    FundQuoteMetadata, IndividualPrice, IndividualPriceAvailability, IndividualPriceFallback,
    MarketDataValuation, StockInfo, ValuationMarketData, ValuationMarketDataAvailability,
};
use crate::services::clock::{Clock, SystemClock};
use crate::services::metrics;

pub use sources::{DefaultMarketDataSources, MarketDataSources, SourceObservation};

pub(crate) struct NavValuationData {
    asset_prices: HashMap<i32, BTreeMap<NaiveDate, f64>>,
    exchange_rates: HashMap<String, BTreeMap<NaiveDate, f64>>,
}

impl NavValuationData {
    pub(crate) fn from_maps(
        asset_prices: HashMap<i32, BTreeMap<NaiveDate, f64>>,
        exchange_rates: HashMap<String, BTreeMap<NaiveDate, f64>>,
    ) -> Self {
        Self {
            asset_prices,
            exchange_rates,
        }
    }

    pub(crate) fn valuation(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> anyhow::Result<MarketDataValuation> {
        let native_price = self
            .asset_prices
            .get(&asset.id)
            .and_then(|prices| prices.range(..=date).next_back())
            .map(|(_, price)| *price)
            .with_context(|| {
                format!(
                    "missing required historical market data for asset {} ({})",
                    asset.ticker, asset.name
                )
            })?;
        let fx_rate = self.exchange_rate_for_asset(asset, date)?;

        Ok(MarketDataValuation {
            native_price,
            fx_rate,
            base_currency_price: native_price * fx_rate,
        })
    }

    pub(crate) fn exchange_rate_for_asset(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> anyhow::Result<f64> {
        if asset.currency == crate::constants::BASE_CURRENCY {
            return Ok(1.0);
        }

        self.exchange_rates
            .get(&asset.currency)
            .and_then(|rates| rates.range(..=date).next_back())
            .map(|(_, rate)| *rate)
            .with_context(|| {
                format!(
                    "missing required historical market data for FX rate for asset {} ({})",
                    asset.ticker, asset.name
                )
            })
    }

    pub(crate) fn valuation_limitations(
        &self,
        asset: &Asset,
        date: NaiveDate,
    ) -> Vec<crate::models::MarketDataLimitation> {
        let mut limitations = Vec::new();
        if self
            .asset_prices
            .get(&asset.id)
            .is_none_or(|prices| prices.range(..=date).next_back().is_none())
        {
            limitations.push(policy::missing_asset_limitation(asset, date));
        }
        if asset.currency != crate::constants::BASE_CURRENCY
            && self
                .exchange_rates
                .get(&asset.currency)
                .is_none_or(|rates| rates.range(..=date).next_back().is_none())
        {
            limitations.push(policy::missing_fx_limitation(&asset.currency, date));
        }
        limitations
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum HistoricalRequest {
    Stock(String, NaiveDate, NaiveDate),
    Fund(String, NaiveDate, NaiveDate),
    Fx(String, String, NaiveDate, NaiveDate),
}

type HistoricalRequestResult = Result<Arc<Vec<SourceObservation>>, Arc<String>>;
type HistoricalRequestFuture = Shared<BoxFuture<'static, HistoricalRequestResult>>;

const HISTORICAL_SOURCE_CONCURRENCY_LIMIT: usize = 4;

pub struct MarketData {
    sources: Arc<dyn MarketDataSources>,
    historical_requests: Mutex<HashMap<HistoricalRequest, HistoricalRequestFuture>>,
    completed_historical_requests: Mutex<HashSet<HistoricalRequest>>,
    historical_source_slots: Arc<Semaphore>,
    today: NaiveDate,
}

impl MarketData {
    pub fn new(sources: Box<dyn MarketDataSources>) -> Self {
        Self::new_with_clock(sources, &SystemClock)
    }

    pub fn new_with_clock(sources: Box<dyn MarketDataSources>, clock: &dyn Clock) -> Self {
        Self {
            sources: sources.into(),
            historical_requests: Mutex::new(HashMap::new()),
            completed_historical_requests: Mutex::new(HashSet::new()),
            historical_source_slots: Arc::new(Semaphore::new(HISTORICAL_SOURCE_CONCURRENCY_LIMIT)),
            // Capture the date once per command's MarketData instance. A command that crosses
            // midnight must not combine different definitions of today across portfolio, NAV,
            // and individual-price work.
            today: clock.today(),
        }
    }

    pub fn today(&self) -> NaiveDate {
        self.today
    }

    pub(crate) async fn stock_price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.request_historical_data(HistoricalRequest::Stock(ticker.to_owned(), start, end))
            .await
    }

    pub(crate) async fn fund_price_history(
        &self,
        code: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.request_historical_data(HistoricalRequest::Fund(code.to_owned(), start, end))
            .await
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

        self.request_historical_data(HistoricalRequest::Fx(from, to, start, end))
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

    #[allow(dead_code)]
    pub async fn prepare_valuation_market_data_if_available(
        &self,
        db: &DatabaseConnection,
        assets: &[Asset],
        start_date: &str,
        end_date: &str,
    ) -> anyhow::Result<ValuationMarketDataAvailability> {
        historical::prepare_valuation_market_data_if_available(
            db, assets, start_date, end_date, self,
        )
        .await
    }

    pub(crate) async fn prepare_valuation_market_data_for_nav(
        &self,
        db: &DatabaseConnection,
        assets: &[Asset],
        start_date: &str,
        end_date: &str,
    ) -> anyhow::Result<(ValuationMarketDataAvailability, NavValuationData)> {
        historical::prepare_valuation_market_data_for_nav(db, assets, start_date, end_date, self)
            .await
    }

    #[allow(dead_code)]
    pub async fn get_required_asset_exchange_rates(
        &self,
        db: &DatabaseConnection,
        assets: &[Asset],
        date: &str,
    ) -> anyhow::Result<std::collections::HashMap<i32, f64>> {
        historical::get_required_asset_exchange_rates(db, assets, date).await
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    pub async fn individual_price(
        &self,
        db: &DatabaseConnection,
        asset: &Asset,
        fallback: IndividualPriceFallback,
    ) -> anyhow::Result<IndividualPrice> {
        individual_price::get_individual_price(db, asset, fallback, self).await
    }

    pub async fn prepare_individual_price_market_data(
        &self,
        db: &DatabaseConnection,
        assets: &[Asset],
        start_date: &str,
        end_date: &str,
    ) -> anyhow::Result<()> {
        individual_price::prepare_individual_price_market_data(
            db, assets, start_date, end_date, self,
        )
        .await
    }

    pub async fn individual_price_if_available(
        &self,
        db: &DatabaseConnection,
        asset: &Asset,
    ) -> anyhow::Result<IndividualPriceAvailability> {
        individual_price::get_individual_price_if_available(db, asset, self).await
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

    pub(crate) fn clear_completed_historical_requests(&self) -> anyhow::Result<()> {
        let completed = {
            let mut completed = self.completed_historical_requests.lock().map_err(|_| {
                anyhow::anyhow!("completed historical request cache mutex poisoned")
            })?;
            completed.drain().collect::<Vec<_>>()
        };
        let mut requests = self
            .historical_requests
            .lock()
            .map_err(|_| anyhow::anyhow!("historical request cache mutex poisoned"))?;
        for request in completed {
            requests.remove(&request);
        }
        Ok(())
    }

    async fn request_historical_data(
        &self,
        request: HistoricalRequest,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        let request_key = request.clone();
        let future = {
            let mut requests = self
                .historical_requests
                .lock()
                .map_err(|_| anyhow::anyhow!("historical request cache mutex poisoned"))?;
            requests
                .entry(request_key.clone())
                .or_insert_with(|| {
                    let sources = Arc::clone(&self.sources);
                    let source_slots = Arc::clone(&self.historical_source_slots);
                    let request = request.clone();
                    async move {
                        let _permit = source_slots
                            .acquire_owned()
                            .await
                            .map_err(|error| Arc::new(error.to_string()))?;
                        let result = match request {
                            HistoricalRequest::Stock(ticker, start, end) => {
                                sources.stock_price_history(&ticker, start, end).await
                            }
                            HistoricalRequest::Fund(code, start, end) => {
                                sources.fund_price_history(&code, start, end).await
                            }
                            HistoricalRequest::Fx(from, to, start, end) => {
                                sources.exchange_rate_history(&from, &to, start, end).await
                            }
                        };
                        result
                            .map(Arc::new)
                            .map_err(|error| Arc::new(format!("{error:#}")))
                    }
                    .boxed()
                    .shared()
                })
                .clone()
        };

        let result = future.await;
        if result.is_ok() {
            self.completed_historical_requests
                .lock()
                .map_err(|_| anyhow::anyhow!("completed historical request cache mutex poisoned"))?
                .insert(request_key);
        }

        result
            .map(|observations| observations.as_ref().clone())
            .map_err(|error| anyhow::anyhow!(error.as_str().to_owned()))
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
