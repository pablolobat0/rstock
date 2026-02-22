use std::path::PathBuf;

use anyhow::{bail, Context};
use yfinance_rs::core::conversions::money_to_f64;
use yfinance_rs::{Ticker, YfClient};

use crate::models::FundPriceResponse;
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
