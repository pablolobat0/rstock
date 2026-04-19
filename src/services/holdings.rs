use anyhow::Context;

use crate::models::FundHolding;
use crate::utils::resolve_scripts_dir;

pub async fn fetch_fund_holdings(identifier: &str, limit: u32) -> anyhow::Result<Vec<FundHolding>> {
    let scripts_dir = resolve_scripts_dir()?;
    let script = scripts_dir.join("get_fund_holdings.py");

    let output = tokio::process::Command::new("uv")
        .arg("run")
        .arg(&script)
        .arg(identifier)
        .arg(limit.to_string())
        .output()
        .await
        .context("failed to run get_fund_holdings.py via uv")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("get_fund_holdings.py failed for {identifier}: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: Vec<serde_json::Value> = serde_json::from_str(stdout.trim())
        .context("failed to parse get_fund_holdings.py output")?;

    let results = parsed
        .iter()
        .map(|entry| FundHolding {
            name: entry["securityName"].as_str().unwrap_or("").to_owned(),
            weighting: entry["weighting"].as_f64().unwrap_or(0.0),
            ticker: entry["ticker"].as_str().map(str::to_owned),
            sector: entry["sector"].as_str().map(str::to_owned),
            country: entry["country"].as_str().map(str::to_owned),
            currency: entry["currency"].as_str().map(str::to_owned),
        })
        .collect();

    Ok(results)
}
