use anyhow::{bail, Context};
use chrono::{NaiveDate, TimeZone, Utc};
use yfinance_rs::core::conversions::money_to_f64;
use yfinance_rs::history::HistoryBuilder;
use yfinance_rs::{Ticker, YfClient};

#[async_trait::async_trait]
pub trait PriceFetcher: Send + Sync {
    async fn get_last_price(&self, ticker: &str, asset_type: &str) -> anyhow::Result<f64>;
    async fn get_historical_prices(
        &self,
        ticker: &str,
        start: &str,
        end: &str,
        asset_type: &str,
    ) -> anyhow::Result<Vec<(String, f64)>>;
}

pub struct RealPriceFetcher;

#[async_trait::async_trait]
impl PriceFetcher for RealPriceFetcher {
    async fn get_last_price(&self, ticker: &str, asset_type: &str) -> anyhow::Result<f64> {
        match asset_type {
            "fund" | "etf" => get_fund_last_price(ticker).await,
            _ => get_stock_last_price(ticker).await,
        }
    }
    async fn get_historical_prices(
        &self,
        ticker: &str,
        start: &str,
        end: &str,
        asset_type: &str,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        match asset_type {
            "fund" | "etf" => get_fund_historical_prices(ticker, start, end).await,
            _ => get_stock_historical_prices(ticker, start, end).await,
        }
    }
}

// --- Scripts directory resolution ---

fn resolve_scripts_dir() -> anyhow::Result<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("RSTOCK_SCRIPTS_DIR") {
        let path = std::path::PathBuf::from(dir);
        if path.is_dir() {
            return Ok(path);
        }
        bail!("RSTOCK_SCRIPTS_DIR is set but not a valid directory: {}", path.display());
    }

    // Walk up from the executable looking for a scripts/ folder
    let exe = std::env::current_exe().context("cannot determine executable path")?;
    let mut dir = exe.parent();
    while let Some(d) = dir {
        let candidate = d.join("scripts");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        dir = d.parent();
    }

    bail!("could not find scripts/ directory (set RSTOCK_SCRIPTS_DIR to override)")
}

// --- Fund/ETF via Python scripts ---

async fn get_fund_last_price(identifier: &str) -> anyhow::Result<f64> {
    let scripts_dir = resolve_scripts_dir()?;
    let script = scripts_dir.join("get_fund_price.py");

    let output = tokio::process::Command::new("uv")
        .arg("run")
        .arg(&script)
        .arg(identifier)
        .output()
        .await
        .context("failed to run get_fund_price.py via uv")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("get_fund_price.py failed for {identifier}: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).context("failed to parse get_fund_price.py output")?;

    let price = parsed["price"]
        .as_f64()
        .context("missing or invalid 'price' in get_fund_price.py output")?;

    if price <= 0.0 {
        bail!("invalid fund price for {identifier}: {price}");
    }

    Ok(price)
}

async fn get_fund_historical_prices(
    identifier: &str,
    start: &str,
    end: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let scripts_dir = resolve_scripts_dir()?;
    let script = scripts_dir.join("get_fund_price_history.py");

    let output = tokio::process::Command::new("uv")
        .arg("run")
        .arg(&script)
        .arg(identifier)
        .arg(start)
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

async fn get_stock_last_price(ticker: &str) -> anyhow::Result<f64> {
    let client = YfClient::default();
    let tk = Ticker::new(&client, ticker);
    let quote = tk.quote().await.context("failed to fetch quote")?;
    let price = quote.price.as_ref().context("no price available")?;
    let value = money_to_f64(price);
    if value <= 0.0 {
        bail!("invalid price for {ticker}: {value}");
    }
    Ok(value)
}

async fn get_stock_historical_prices(
    ticker: &str,
    start: &str,
    end: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let start_date = NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .context("invalid start date")?;
    let end_date = NaiveDate::parse_from_str(end, "%Y-%m-%d")
        .context("invalid end date")?;

    let start_dt = Utc.from_utc_datetime(
        &start_date.and_hms_opt(0, 0, 0).unwrap(),
    );
    let end_dt = Utc.from_utc_datetime(
        &end_date.and_hms_opt(23, 59, 59).unwrap(),
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
            let date_str = candle.ts.format("%Y-%m-%d").to_string();
            let price = money_to_f64(&candle.close);
            (date_str, price)
        })
        .collect();

    Ok(results)
}
