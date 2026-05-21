use anyhow::Context;
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, BASE_CURRENCY};
use crate::db::repos::{daily_price_repo, exchange_rate_repo};
use crate::models::{Asset, AssetDisplayMarketData, AssetType, MarketDataLimitation};
use crate::services::market_data_policy;
use crate::services::price::PriceFetcher;

pub async fn get_asset_display_market_data(
    db: &DatabaseConnection,
    asset: &Asset,
    fallback_native_price: f64,
    fallback_price_date: &str,
    fallback_fx_rate: f64,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<AssetDisplayMarketData> {
    let today = chrono::Local::now().date_naive();
    let today_str = format_date(today);
    let yesterday_str = format_date(today - chrono::Duration::days(1));

    let (native_price, price_date, price_limitation) = get_display_price(
        db,
        asset,
        &today_str,
        &yesterday_str,
        fallback_native_price,
        fallback_price_date,
        price_fetcher,
    )
    .await?;
    let (fx_rate, mut limitations) = get_display_exchange_rate(
        db,
        asset,
        &today_str,
        &yesterday_str,
        fallback_fx_rate,
        price_fetcher,
    )
    .await?;
    if let Some(limitation) = price_limitation {
        limitations.push(limitation);
    }

    Ok(AssetDisplayMarketData {
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
    fallback_native_price: f64,
    fallback_price_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<(f64, String, Option<MarketDataLimitation>)> {
    if asset.asset_type == AssetType::Stock {
        if let Some(live_price) = fetch_live_asset_price(asset, today, price_fetcher)
            .await
            .unwrap_or(None)
        {
            return Ok((live_price, today.to_owned(), None));
        }
    }

    let (price, date) =
        match daily_price_repo::find_price_and_date_at_or_before(db, asset.id, yesterday).await? {
            Some((price, date)) => (price, date),
            None => (fallback_native_price, fallback_price_date.to_owned()),
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
    fallback_fx_rate: f64,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<(f64, Vec<MarketDataLimitation>)> {
    if asset.currency == BASE_CURRENCY {
        return Ok((1.0, Vec::new()));
    }

    let pair = market_data_policy::currency_pair(&asset.currency);
    if let Some(live_rate) = fetch_live_exchange_rate(&pair, today, price_fetcher)
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
        return Ok((fallback_fx_rate, Vec::new()));
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
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<f64>> {
    let prices = price_fetcher
        .get_historical_prices(&asset.ticker, date, date, &AssetType::Stock)
        .await
        .with_context(|| format!("failed to fetch live price for {}", asset.ticker))?;
    Ok(prices.last().map(|(_, price)| *price))
}

async fn fetch_live_exchange_rate(
    pair: &str,
    date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<f64>> {
    let rates = price_fetcher
        .get_historical_exchange_rates(pair, date, date)
        .await
        .with_context(|| format!("failed to fetch live exchange rate for {pair}"))?;
    Ok(rates.last().map(|(_, rate)| *rate))
}
