use sea_orm::DatabaseConnection;

use crate::constants::{format_date, BASE_CURRENCY};
use crate::db::repos::{daily_price_repo, exchange_rate_repo};
use crate::models::{
    Asset, AssetType, IndividualPrice, IndividualPriceFallback, MarketDataLimitation,
};
use crate::services::market_data::MarketData;
use crate::services::market_data_policy;

pub(crate) async fn get_individual_price(
    db: &DatabaseConnection,
    asset: &Asset,
    fallback: IndividualPriceFallback,
    market_data: &MarketData,
) -> anyhow::Result<IndividualPrice> {
    let today = chrono::Local::now().date_naive();
    let today_str = format_date(today);
    let yesterday_str = format_date(today - chrono::Duration::days(1));

    let (native_price, price_date, price_limitation) = get_display_price(
        db,
        asset,
        &today_str,
        &yesterday_str,
        &fallback,
        market_data,
    )
    .await?;
    let (fx_rate, mut limitations) = get_display_exchange_rate(
        db,
        asset,
        &today_str,
        &yesterday_str,
        &fallback,
        market_data,
    )
    .await?;
    if let Some(limitation) = price_limitation {
        limitations.push(limitation);
    }

    Ok(IndividualPrice {
        native_price,
        price_date,
        fx_rate,
        base_currency_price: native_price * fx_rate,
        limitations,
    })
}

async fn get_display_price(
    db: &DatabaseConnection,
    asset: &Asset,
    today: &str,
    yesterday: &str,
    fallback: &IndividualPriceFallback,
    market_data: &MarketData,
) -> anyhow::Result<(f64, String, Option<MarketDataLimitation>)> {
    if asset.asset_type == AssetType::Stock {
        if let Some(live_price) = fetch_live_asset_price(asset, today, market_data)
            .await
            .unwrap_or(None)
        {
            return Ok((live_price, today.to_owned(), None));
        }
    }

    let (price, date) =
        match daily_price_repo::find_price_and_date_at_or_before(db, asset.id, yesterday).await? {
            Some((price, date)) => (price, date),
            None => (fallback.native_price, fallback.price_date.clone()),
        };
    let available_on = market_data_policy::parse_market_data_date(&date, "asset price date")?;
    let requested_end =
        market_data_policy::parse_market_data_date(yesterday, "display asset price end date")?;
    let limitation =
        market_data_policy::classify_asset_limitation(asset, available_on, requested_end);

    Ok((price, date, limitation))
}

async fn get_display_exchange_rate(
    db: &DatabaseConnection,
    asset: &Asset,
    today: &str,
    yesterday: &str,
    fallback: &IndividualPriceFallback,
    market_data: &MarketData,
) -> anyhow::Result<(f64, Vec<MarketDataLimitation>)> {
    if asset.currency == BASE_CURRENCY {
        return Ok((1.0, Vec::new()));
    }

    if let Some(live_rate) = fetch_live_exchange_rate(&asset.currency, today, market_data)
        .await
        .unwrap_or(None)
    {
        return Ok((live_rate, Vec::new()));
    }

    let Some((rate, date_string)) = exchange_rate_repo::find_rate_and_date_at_or_before(
        db,
        &asset.currency,
        BASE_CURRENCY,
        yesterday,
    )
    .await?
    else {
        return Ok((fallback.fx_rate, Vec::new()));
    };
    let available_on = market_data_policy::parse_market_data_date(&date_string, "FX rate date")?;
    let requested_end =
        market_data_policy::parse_market_data_date(yesterday, "display FX end date")?;
    let limitations =
        market_data_policy::classify_fx_limitation(&asset.currency, available_on, requested_end)
            .into_iter()
            .collect();

    Ok((rate, limitations))
}

async fn fetch_live_asset_price(
    asset: &Asset,
    date: &str,
    market_data: &MarketData,
) -> anyhow::Result<Option<f64>> {
    let date = market_data_policy::parse_market_data_date(date, "live asset price date")?;
    let prices = market_data
        .stock_price_history(&asset.ticker, date, date)
        .await?;
    Ok(prices.last().map(|observation| observation.value))
}

async fn fetch_live_exchange_rate(
    currency: &str,
    date: &str,
    market_data: &MarketData,
) -> anyhow::Result<Option<f64>> {
    let date = market_data_policy::parse_market_data_date(date, "live FX date")?;
    let rates = market_data
        .exchange_rate_history(currency, BASE_CURRENCY, date, date)
        .await?;
    Ok(rates.last().map(|observation| observation.value))
}
