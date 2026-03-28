use anyhow::Context;
use sea_orm::DatabaseConnection;

use crate::models::{AssetType, DirectHolding, FundHolding, FundWithHoldings, HoldingsResult};
use crate::services::portfolio::get_portfolio;
use crate::utils::resolve_scripts_dir;

pub async fn get_holdings(db: &DatabaseConnection) -> anyhow::Result<HoldingsResult> {
    let portfolio = get_portfolio(db).await?;
    let total_value = portfolio.total_current_value;

    let mut stocks = Vec::new();
    let mut funds = Vec::new();

    for pos in &portfolio.rows {
        let weight = if total_value > 0.0 {
            (pos.current_value / total_value) * 100.0
        } else {
            0.0
        };

        match pos.asset_type {
            AssetType::Stock => {
                stocks.push(DirectHolding {
                    ticker: pos.ticker.clone(),
                    name: pos.name.clone(),
                    portfolio_weight: weight,
                    current_value: pos.current_value,
                });
            }
            AssetType::Fund | AssetType::Etf => {
                let identifier = pos.isin.as_deref().unwrap_or(&pos.ticker);
                let (holdings, error) = match fetch_fund_holdings(identifier).await {
                    Ok(h) => (h, None),
                    Err(e) => (Vec::new(), Some(format!("{e:#}"))),
                };
                funds.push(FundWithHoldings {
                    ticker: pos.ticker.clone(),
                    name: pos.name.clone(),
                    portfolio_weight: weight,
                    current_value: pos.current_value,
                    holdings,
                    error,
                });
            }
        }
    }

    Ok(HoldingsResult {
        stocks,
        funds,
        total_portfolio_value: total_value,
    })
}

async fn fetch_fund_holdings(identifier: &str) -> anyhow::Result<Vec<FundHolding>> {
    let scripts_dir = resolve_scripts_dir()?;
    let script = scripts_dir.join("get_fund_holdings.py");

    let output = tokio::process::Command::new("uv")
        .arg("run")
        .arg(&script)
        .arg(identifier)
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
            ticker: entry["ticker"].as_str().unwrap_or("").to_owned(),
            name: entry["name"].as_str().unwrap_or("").to_owned(),
            weighting: entry["weighting"].as_f64().unwrap_or(0.0),
        })
        .collect();

    Ok(results)
}
