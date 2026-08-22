use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use super::{historical, policy, MarketData};
use crate::constants::{format_date, BASE_CURRENCY};
use crate::db::repos::{daily_price_repo, exchange_rate_repo};
use crate::models::{Asset, AssetType, IndividualPriceAvailability};

pub(crate) async fn get_individual_price_if_available(
    db: &DatabaseConnection,
    asset: &Asset,
    market_data: &MarketData,
) -> anyhow::Result<IndividualPriceAvailability> {
    let today = market_data.today();
    let today_str = format_date(today);
    let yesterday = today - chrono::Duration::days(1);
    let yesterday_str = format_date(yesterday);
    let mut limitations = Vec::new();

    let (price_result, fx_result) = tokio::join!(
        fetch_same_day_asset_price(asset, today, market_data),
        fetch_live_exchange_rate_if_needed(asset, &today_str, market_data),
    );

    let price = match price_result {
        Ok(price) => price.map(|price| (price, today_str.clone())),
        Err(error) => {
            tracing::warn!(ticker = %asset.ticker, error = %error, "failed to fetch live asset price");
            None
        }
    };
    let price = match price {
        Some(price) => Some(price),
        None => {
            daily_price_repo::find_price_and_date_at_or_before(db, asset.id, &yesterday_str).await?
        }
    };

    if let Some((_, date)) = &price {
        let available_on = policy::parse_market_data_date(date, "asset price date")?;
        if let Some(limitation) = policy::classify_asset_limitation(asset, available_on, yesterday)
        {
            limitations.push(limitation);
        }
    } else {
        limitations.push(policy::missing_asset_limitation(asset, yesterday));
    }

    let fx_rate = if let Some(rate) = match fx_result {
        Ok(rate) => rate,
        Err(error) => {
            tracing::warn!(currency = %asset.currency, error = %error, "failed to fetch live exchange rate");
            None
        }
    } {
        Some(rate)
    } else {
        let cached = exchange_rate_repo::find_rate_and_date_at_or_before(
            db,
            &asset.currency,
            BASE_CURRENCY,
            &yesterday_str,
        )
        .await?;
        if let Some((rate, date)) = cached {
            let available_on = policy::parse_market_data_date(&date, "FX rate date")?;
            if let Some(limitation) =
                policy::classify_fx_limitation(&asset.currency, available_on, yesterday)
            {
                limitations.push(limitation);
            }
            Some(rate)
        } else {
            limitations.push(policy::missing_fx_limitation(&asset.currency, yesterday));
            None
        }
    };

    Ok(IndividualPriceAvailability {
        native_price: price.as_ref().map(|(price, _)| *price),
        price_date: price.map(|(_, date)| date),
        fx_rate,
        limitations,
    })
}

async fn fetch_live_exchange_rate_if_needed(
    asset: &Asset,
    date: &str,
    market_data: &MarketData,
) -> anyhow::Result<Option<f64>> {
    if asset.currency == BASE_CURRENCY {
        return Ok(Some(1.0));
    }
    fetch_live_exchange_rate(&asset.currency, date, market_data).await
}

pub(crate) async fn prepare_individual_price_market_data(
    db: &DatabaseConnection,
    assets: &[Asset],
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<()> {
    historical::fill_historical_market_data_cache(db, assets, start_date, end_date, market_data)
        .await
}

async fn fetch_same_day_asset_price(
    asset: &Asset,
    date: NaiveDate,
    market_data: &MarketData,
) -> anyhow::Result<Option<f64>> {
    let observations = match asset.asset_type {
        AssetType::Stock => {
            market_data
                .stock_price_history(&asset.ticker, date, date)
                .await?
        }
        AssetType::Etf => {
            let code = asset.morningstar_code.as_deref().with_context(|| {
                format!(
                    "missing Morningstar code for required ETF {} ({})",
                    asset.ticker, asset.name
                )
            })?;
            market_data.fund_price_history(code, date, date).await?
        }
        AssetType::Fund => return Ok(None),
    };

    Ok(observations
        .into_iter()
        .find(|observation| observation.date == date)
        .map(|observation| observation.value))
}

async fn fetch_live_exchange_rate(
    currency: &str,
    date: &str,
    market_data: &MarketData,
) -> anyhow::Result<Option<f64>> {
    let date = policy::parse_market_data_date(date, "live FX date")?;
    let rates = market_data
        .exchange_rate_history(currency, BASE_CURRENCY, date, date)
        .await?;
    Ok(rates.last().map(|observation| observation.value))
}
