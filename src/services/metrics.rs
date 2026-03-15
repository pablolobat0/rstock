use sea_orm::DatabaseConnection;
use std::collections::HashMap;

use crate::db::repos::{asset_repo, daily_price_repo, portfolio_history_repo};
use crate::models::{AssetInfo, AssetType};
use crate::services::daily_prices;
use crate::services::price::PriceFetcher;

const BENCHMARK_TICKER: &str = "ACWI";
const BENCHMARK_NAME: &str = "MSCI ACWI Benchmark";
const ANNUAL_RISK_FREE_RATE: f64 = 0.03;
const TRADING_DAYS_PER_YEAR: f64 = 252.0;
const MIN_DATA_POINTS: usize = 20;

pub fn is_benchmark_ticker(ticker: &str) -> bool {
    ticker == BENCHMARK_TICKER
}

pub async fn compute_risk_metrics(
    db: &DatabaseConnection,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<(f64, f64)>> {
    let today = chrono::Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);
    let one_year_ago = today - chrono::Duration::days(365);

    let yesterday_str = yesterday.format("%Y-%m-%d").to_string();
    let one_year_ago_str = one_year_ago.format("%Y-%m-%d").to_string();

    // Ensure benchmark asset exists and prices are cached
    let benchmark_asset_id =
        ensure_benchmark_prices(db, &one_year_ago_str, &yesterday_str, price_fetcher).await?;

    // Get portfolio NAV snapshots for the trailing 1Y
    let nav_snapshots =
        portfolio_history_repo::find_between(db, &one_year_ago_str, &yesterday_str).await?;
    if nav_snapshots.len() < MIN_DATA_POINTS {
        return Ok(None);
    }

    // Get benchmark prices for the same period
    let benchmark_prices = daily_price_repo::find_prices_between(
        db,
        benchmark_asset_id,
        &one_year_ago_str,
        &yesterday_str,
    )
    .await?;

    // Build a date->price map for the benchmark
    let benchmark_map: HashMap<&str, f64> = benchmark_prices
        .iter()
        .map(|(d, p)| (d.as_str(), *p))
        .collect();

    // Align dates and compute daily log returns for both series
    let mut portfolio_returns = Vec::new();
    let mut benchmark_returns = Vec::new();

    for i in 1..nav_snapshots.len() {
        let date = &nav_snapshots[i].date;
        let prev_date = &nav_snapshots[i - 1].date;

        if let (Some(&bench_today), Some(&bench_prev)) = (
            benchmark_map.get(date.as_str()),
            benchmark_map.get(prev_date.as_str()),
        ) {
            if bench_prev > 0.0 && nav_snapshots[i - 1].nav > 0.0 {
                let bench_ret = (bench_today / bench_prev).ln();
                // Skip non-trading days (weekends/holidays where prices are forward-filled)
                if bench_ret.abs() < f64::EPSILON {
                    continue;
                }
                let port_ret = (nav_snapshots[i].nav / nav_snapshots[i - 1].nav).ln();
                portfolio_returns.push(port_ret);
                benchmark_returns.push(bench_ret);
            }
        }
    }

    if portfolio_returns.len() < MIN_DATA_POINTS {
        return Ok(None);
    }

    let n = portfolio_returns.len() as f64;

    // Sharpe ratio
    let daily_rf = (1.0 + ANNUAL_RISK_FREE_RATE).powf(1.0 / TRADING_DAYS_PER_YEAR) - 1.0;
    let mean_port: f64 = portfolio_returns.iter().sum::<f64>() / n;
    let excess_returns: Vec<f64> = portfolio_returns.iter().map(|r| r - daily_rf).collect();
    let mean_excess: f64 = excess_returns.iter().sum::<f64>() / n;
    let var_port: f64 = excess_returns
        .iter()
        .map(|r| (r - mean_excess).powi(2))
        .sum::<f64>()
        / (n - 1.0);
    let std_port = var_port.sqrt();
    let sharpe = if std_port > 0.0 {
        (mean_excess / std_port) * TRADING_DAYS_PER_YEAR.sqrt()
    } else {
        0.0
    };

    // Beta
    let mean_bench: f64 = benchmark_returns.iter().sum::<f64>() / n;
    let cov: f64 = portfolio_returns
        .iter()
        .zip(benchmark_returns.iter())
        .map(|(p, b)| (p - mean_port) * (b - mean_bench))
        .sum::<f64>()
        / (n - 1.0);
    let var_bench: f64 = benchmark_returns
        .iter()
        .map(|b| (b - mean_bench).powi(2))
        .sum::<f64>()
        / (n - 1.0);
    let beta = if var_bench > 0.0 {
        cov / var_bench
    } else {
        0.0
    };

    Ok(Some((beta, sharpe)))
}

async fn ensure_benchmark_prices(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<i32> {
    let info = AssetInfo {
        ticker: BENCHMARK_TICKER.to_owned(),
        name: BENCHMARK_NAME.to_owned(),
        asset_type: AssetType::Stock,
        isin: None,
        currency: "USD".to_owned(),
    };

    let asset_id = asset_repo::get_or_create(db, &info).await?;

    let asset = crate::models::Asset {
        id: asset_id,
        ticker: BENCHMARK_TICKER.to_owned(),
        isin: None,
        name: BENCHMARK_NAME.to_owned(),
        asset_type: AssetType::Stock,
        currency: "USD".to_owned(),
    };

    daily_prices::fill_prices_for_range(db, &asset, start_date, end_date, price_fetcher).await?;

    Ok(asset_id)
}
