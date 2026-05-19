use anyhow::Context;
use chrono::{Datelike, NaiveDate, Weekday};

use crate::constants::{BASE_CURRENCY, DATE_FORMAT};
use crate::models::{
    Asset, AssetType, MarketDataLimitation, MarketDataLimitationClassification, MarketDataSubject,
};

const STALE_COMPLETED_WEEKDAY_WARNING_THRESHOLD: u32 = 4;

pub(crate) fn currency_pair(from: &str) -> String {
    format!("{from}{BASE_CURRENCY}")
}

pub(crate) fn classify_asset_limitation(
    asset: &Asset,
    latest_date: NaiveDate,
    requested_end: NaiveDate,
) -> Option<MarketDataLimitation> {
    if latest_date >= requested_end {
        return None;
    }

    let classification = match asset.asset_type {
        AssetType::Fund | AssetType::Etf => {
            if (requested_end - latest_date).num_days() <= 7 {
                return None;
            }
            MarketDataLimitationClassification::ActionableReportingLag
        }
        AssetType::Stock => {
            if completed_weekdays_between(latest_date, requested_end)
                < STALE_COMPLETED_WEEKDAY_WARNING_THRESHOLD
            {
                return None;
            }
            MarketDataLimitationClassification::ActionableStaleData
        }
    };

    Some(MarketDataLimitation {
        subject: MarketDataSubject::Asset {
            ticker: asset.ticker.clone(),
            name: asset.name.clone(),
            asset_type: asset.asset_type.clone(),
        },
        latest_available_date: latest_date,
        requested_end_date: requested_end,
        classification,
    })
}

pub(crate) fn classify_fx_limitation(
    currency: &str,
    latest_date: NaiveDate,
    requested_end: NaiveDate,
) -> Option<MarketDataLimitation> {
    if latest_date >= requested_end
        || completed_weekdays_between(latest_date, requested_end)
            < STALE_COMPLETED_WEEKDAY_WARNING_THRESHOLD
    {
        return None;
    }

    Some(MarketDataLimitation {
        subject: MarketDataSubject::FxRate {
            currency: currency.to_owned(),
        },
        latest_available_date: latest_date,
        requested_end_date: requested_end,
        classification: MarketDataLimitationClassification::ActionableStaleData,
    })
}

pub(crate) fn parse_market_data_date(date: &str, label: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).with_context(|| format!("invalid {label}: {date}"))
}

fn completed_weekdays_between(latest_date: NaiveDate, requested_end: NaiveDate) -> u32 {
    let mut count = 0;
    let mut current = latest_date + chrono::Duration::days(1);
    while current <= requested_end {
        if !matches!(current.weekday(), Weekday::Sat | Weekday::Sun) {
            count += 1;
        }
        current += chrono::Duration::days(1);
    }
    count
}
