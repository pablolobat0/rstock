use anyhow::{bail, Context};
use chrono::{NaiveDate, TimeZone, Utc};
use yfinance_rs::core::conversions::money_to_f64;
use yfinance_rs::history::HistoryBuilder;
use yfinance_rs::profile::{self, Profile};
use yfinance_rs::ticker::Ticker;
use yfinance_rs::YfClient;

use crate::constants::DATE_FORMAT;
use crate::models::monitor::StockInfo;
use crate::models::AssetType;
use crate::utils::resolve_scripts_dir;

#[async_trait::async_trait]
pub trait PriceFetcher: Send + Sync {
    async fn get_historical_prices(
        &self,
        ticker: &str,
        start: &str,
        end: &str,
        asset_type: &AssetType,
    ) -> anyhow::Result<Vec<(String, f64)>>;

    async fn get_historical_exchange_rates(
        &self,
        pair: &str,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<(String, f64)>>;

    async fn get_stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo>;
}

pub struct RealPriceFetcher;

#[async_trait::async_trait]
impl PriceFetcher for RealPriceFetcher {
    async fn get_historical_prices(
        &self,
        ticker: &str,
        start: &str,
        end: &str,
        asset_type: &AssetType,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        tracing::debug!(%ticker, %start, %end, ?asset_type, "fetching historical prices");
        match asset_type {
            AssetType::Fund | AssetType::Etf => {
                get_fund_historical_prices(ticker, start, end).await
            }
            AssetType::Stock => get_stock_historical_prices(ticker, start, end).await,
        }
    }

    async fn get_historical_exchange_rates(
        &self,
        pair: &str,
        start: &str,
        end: &str,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        tracing::debug!(%pair, %start, %end, "fetching exchange rates");
        let ticker = format!("{pair}=X");
        get_stock_historical_prices(&ticker, start, end).await
    }

    async fn get_stock_info(&self, ticker: &str) -> anyhow::Result<StockInfo> {
        tracing::debug!(%ticker, "fetching stock info");
        fetch_stock_info(ticker).await
    }
}

// --- Fund/ETF via Python scripts ---

async fn get_fund_historical_prices(
    identifier: &str,
    start: &str,
    end: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    // Pad start date backward to avoid slow empty API responses when the
    // requested range falls on non-trading days (holidays, API lag).
    let start_date = NaiveDate::parse_from_str(start, DATE_FORMAT).context("invalid start date")?;
    let padded_start = start_date - chrono::Duration::days(crate::constants::FUND_API_PADDING_DAYS);
    let padded_start_str = padded_start.format(DATE_FORMAT).to_string();

    let scripts_dir = resolve_scripts_dir()?;
    let script = scripts_dir.join("get_fund_price_history.py");

    let output = tokio::process::Command::new("uv")
        .arg("run")
        .arg(&script)
        .arg(identifier)
        .arg(&padded_start_str)
        .arg(end)
        .output()
        .await
        .context("failed to run get_fund_price_history.py via uv")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("get_fund_price_history.py failed for {identifier}: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(stdout.trim())
        .context("failed to parse get_fund_price_history.py output")?;

    let results: Vec<(String, f64)> = parsed
        .iter()
        .filter_map(|entry| {
            let date = entry["date"].as_str()?.to_owned();
            let price = entry["price"].as_f64()?;
            Some((date, price))
        })
        .collect();

    Ok(results)
}

// --- Stock via Yahoo Finance ---

async fn get_stock_historical_prices(
    ticker: &str,
    start: &str,
    end: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let start_date = NaiveDate::parse_from_str(start, DATE_FORMAT).context("invalid start date")?;
    let end_date = NaiveDate::parse_from_str(end, DATE_FORMAT).context("invalid end date")?;

    let start_dt =
        Utc.from_utc_datetime(&start_date.and_hms_opt(0, 0, 0).expect("valid HMS constant"));
    let end_dt = Utc.from_utc_datetime(
        &end_date
            .and_hms_opt(23, 59, 59)
            .expect("valid HMS constant"),
    );

    let client = YfClient::default();
    let candles = HistoryBuilder::new(&client, ticker)
        .between(start_dt, end_dt)
        .fetch()
        .await
        .context(format!("failed to fetch historical prices for {ticker}"))?;

    let results: Vec<(String, f64)> = candles
        .iter()
        .map(|candle| {
            let date_str = candle.ts.format(DATE_FORMAT).to_string();
            let price = money_to_f64(&candle.close);
            (date_str, price)
        })
        .collect();

    Ok(results)
}

async fn fetch_stock_info(ticker: &str) -> anyhow::Result<StockInfo> {
    let client = YfClient::default();
    let t = Ticker::new(&client, ticker);
    let info = t
        .info()
        .await
        .context(format!("failed to fetch info for {ticker}"))?;

    let current_price = info.last.as_ref().map(money_to_f64);
    let previous_close = info.previous_close.as_ref().map(money_to_f64);

    let day_range = match (&info.day_range_low, &info.day_range_high) {
        (Some(lo), Some(hi)) => Some((money_to_f64(lo), money_to_f64(hi))),
        _ => None,
    };

    let fifty_two_week_range = match (&info.fifty_two_week_low, &info.fifty_two_week_high) {
        (Some(lo), Some(hi)) => Some((money_to_f64(lo), money_to_f64(hi))),
        _ => None,
    };

    let market_cap = info.market_cap.as_ref().map(money_to_f64);
    let eps_ttm = info.eps_ttm.as_ref().map(money_to_f64);

    let profile_result = profile::load_profile(&client, ticker).await.ok();
    let (sector, industry, country, name_from_profile) = match &profile_result {
        Some(Profile::Company(c)) => (
            c.sector.clone(),
            c.industry.clone(),
            c.address.as_ref().and_then(|a| a.country.clone()),
            Some(c.name.clone()),
        ),
        Some(Profile::Fund(f)) => (None, None, None, Some(f.name.clone())),
        None => (None, None, None, None),
    };

    let name = info.name.clone().or(name_from_profile);
    let currency = info.currency.as_ref().map(std::string::ToString::to_string);

    Ok(StockInfo {
        ticker: ticker.to_owned(),
        name,
        currency,
        current_price,
        previous_close,
        day_range,
        fifty_two_week_range,
        volume: info.volume,
        avg_volume: info.average_volume,
        market_cap,
        pe_ttm: info.pe_ttm,
        eps_ttm,
        dividend_yield: info.dividend_yield,
        sector,
        industry,
        country,
    })
}
