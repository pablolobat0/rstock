use chrono::NaiveDate;

use crate::models::AssetType;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavMarketData {
    pub effective_end: NaiveDate,
    pub limitations: Vec<MarketDataLimitation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketDataLimitation {
    pub subject: MarketDataSubject,
    pub latest_available_date: NaiveDate,
    pub requested_end_date: NaiveDate,
    pub classification: MarketDataLimitationClassification,
    pub source: MarketDataLimitationSource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketDataSubject {
    Asset {
        ticker: String,
        name: String,
        asset_type: AssetType,
    },
    FxRate {
        pair: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketDataLimitationClassification {
    AcceptableReportingLag,
    ActionableReportingLag,
    ActionableStaleData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarketDataLimitationSource {
    CachedFallback,
    SourceLag,
}
