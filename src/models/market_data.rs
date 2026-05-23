use chrono::NaiveDate;

use crate::models::AssetType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValuationMarketData {
    pub effective_end: NaiveDate,
    pub limitations: Vec<MarketDataLimitation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkMarketData {
    pub asset_id: i32,
    pub effective_end: NaiveDate,
    pub limitations: Vec<MarketDataLimitation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MarketDataValuation {
    pub native_price: f64,
    pub fx_rate: f64,
    pub base_currency_price: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndividualPriceFallback {
    pub native_price: f64,
    pub price_date: String,
    pub fx_rate: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IndividualPrice {
    pub native_price: f64,
    pub price_date: String,
    pub fx_rate: f64,
    pub base_currency_price: f64,
    pub limitations: Vec<MarketDataLimitation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketDataLimitation {
    pub subject: MarketDataSubject,
    pub latest_available_date: NaiveDate,
    pub requested_end_date: NaiveDate,
    pub classification: MarketDataLimitationClassification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketDataLimitationClassification {
    ActionableReportingLag,
    ActionableStaleData,
}
