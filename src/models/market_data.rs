use chrono::NaiveDate;
use serde::Serialize;

use crate::models::AssetType;

pub type BaseCurrencyPriceSeries = Vec<(String, f64)>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuationMarketData {
    pub effective_end: NaiveDate,
    pub limitations: Vec<MarketDataLimitation>,
}

/// Result of preparing valuation market data without treating unavailable
/// required data as an error: `data_available` is false when any required asset
/// price or FX rate has no data at all, and the unavailable inputs are reported
/// as `limitations`. Genuine errors (DB, parsing, invariants) still propagate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuationMarketDataAvailability {
    pub effective_end: NaiveDate,
    pub limitations: Vec<MarketDataLimitation>,
    pub data_available: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketDataValuation {
    pub native_price: f64,
    pub fx_rate: f64,
    pub base_currency_price: f64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct IndividualPriceFallback {
    pub native_price: f64,
    pub price_date: String,
    pub fx_rate: f64,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct IndividualPrice {
    pub native_price: f64,
    pub price_date: String,
    pub fx_rate: f64,
    pub base_currency_price: f64,
    pub limitations: Vec<MarketDataLimitation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndividualPriceAvailability {
    pub native_price: Option<f64>,
    pub price_date: Option<String>,
    pub fx_rate: Option<f64>,
    pub limitations: Vec<MarketDataLimitation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorrelationMarketDataSeries {
    pub asset_id: i32,
    pub name: String,
    pub prices: BaseCurrencyPriceSeries,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorrelationMarketData {
    pub requested_start_date: String,
    pub requested_end_date: String,
    pub tracked_asset_series: Vec<CorrelationMarketDataSeries>,
    pub benchmark_series: CorrelationMarketDataSeries,
    pub limitations: Vec<MarketDataLimitation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MarketDataLimitation {
    pub subject: MarketDataSubject,
    pub latest_available_date: Option<NaiveDate>,
    pub requested_end_date: NaiveDate,
    pub classification: MarketDataLimitationClassification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarketDataSubject {
    Asset {
        ticker: String,
        name: String,
        asset_type: AssetType,
    },
    FxRate {
        currency: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(clippy::enum_variant_names)]
pub enum MarketDataLimitationClassification {
    ActionableMissingData,
    ActionableReportingLag,
    ActionableStaleData,
}
