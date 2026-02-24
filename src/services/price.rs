use std::path::PathBuf;

use anyhow::{bail, Context};
use chrono::{NaiveDate, TimeZone, Utc};
use yfinance_rs::core::conversions::money_to_f64;
use yfinance_rs::history::HistoryBuilder;
use yfinance_rs::{Ticker, YfClient};

use crate::models::{FundPriceHistoryEntry, FundPriceResponse};
use crate::settings::Settings;

pub async fn get_last_price(ticker: &str) -> anyhow::Result<f64> {
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

pub async fn get_last_fund_price(identifier: &str) -> anyhow::Result<f64> {
    let script = find_scripts_dir()?.join(Settings::GetFundPriceScript.as_str());

    let output = tokio::process::Command::new("uv")
        .args(["run", &script.to_string_lossy(), identifier])
        .output()
        .await
        .context("failed to spawn uv process")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("get_fund_price.py failed: {}", stderr.trim());
    }

    let resp: FundPriceResponse =
        serde_json::from_slice(&output.stdout).context("failed to parse fund price JSON output")?;

    Ok(resp.price)
}

pub async fn get_historical_stock_prices(
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

pub async fn get_historical_fund_prices(
    identifier: &str,
    start: &str,
    end: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let script = find_scripts_dir()?.join(Settings::GetFundPriceHistoryScript.as_str());

    let output = tokio::process::Command::new("uv")
        .args(["run", &script.to_string_lossy(), identifier, start, end])
        .output()
        .await
        .context("failed to spawn uv process for fund price history")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("get_fund_price_history.py failed: {}", stderr.trim());
    }

    let entries: Vec<FundPriceHistoryEntry> = serde_json::from_slice(&output.stdout)
        .context("failed to parse fund price history JSON")?;

    Ok(entries.into_iter().map(|e| (e.date, e.price)).collect())
}

fn find_scripts_dir() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("RSTOCK_SCRIPTS_DIR") {
        return Ok(PathBuf::from(dir));
    }

    let exe = std::env::current_exe().context("failed to get current executable path")?;
    let mut dir = exe.parent().map(|p| p.to_path_buf());
    while let Some(d) = dir {
        let scripts = d.join(Settings::ScriptsDir.as_str());
        if scripts.is_dir() {
            return Ok(scripts);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }

    bail!("could not find scripts directory; set RSTOCK_SCRIPTS_DIR env var")
}
