use sea_orm::DatabaseConnection;
use std::collections::HashMap;

use crate::constants::{
    format_date, ANNUAL_RISK_FREE_RATE, BENCHMARK_CURRENCY, BENCHMARK_NAME, BENCHMARK_TICKER,
    MIN_DATA_POINTS, ONE_YEAR_DAYS, TRADING_DAYS_PER_YEAR, ZERO_RETURN_THRESHOLD,
};
use crate::db::repos::{asset_repo, daily_price_repo, portfolio_history_repo};
use crate::models::{AssetInfo, AssetType, PortfolioSnapshot};
use crate::services::daily_prices;
use crate::services::price::PriceFetcher;

pub fn is_benchmark_ticker(ticker: &str) -> bool {
    ticker == BENCHMARK_TICKER
}

pub async fn compute_risk_metrics(
    db: &DatabaseConnection,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<(f64, f64)>> {
    let today = chrono::Local::now().date_naive();
    let yesterday = today - chrono::Duration::days(1);
    let one_year_ago = today - chrono::Duration::days(ONE_YEAR_DAYS);

    let yesterday_str = format_date(yesterday);
    let one_year_ago_str = format_date(one_year_ago);

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
    let (portfolio_returns, benchmark_returns) = align_returns(&nav_snapshots, &benchmark_map);

    if portfolio_returns.len() < MIN_DATA_POINTS {
        return Ok(None);
    }

    let sharpe = compute_sharpe(&portfolio_returns);
    let beta = compute_beta(&portfolio_returns, &benchmark_returns);

    Ok(Some((beta, sharpe)))
}

/// Aligns portfolio NAV snapshots with benchmark prices, computing daily log returns
/// and filtering out non-trading days (forward-filled prices).
fn align_returns(
    nav_snapshots: &[PortfolioSnapshot],
    benchmark_map: &HashMap<&str, f64>,
) -> (Vec<f64>, Vec<f64>) {
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
                if bench_ret.abs() < ZERO_RETURN_THRESHOLD {
                    continue;
                }
                let port_ret = (nav_snapshots[i].nav / nav_snapshots[i - 1].nav).ln();
                portfolio_returns.push(port_ret);
                benchmark_returns.push(bench_ret);
            }
        }
    }

    (portfolio_returns, benchmark_returns)
}

/// Computes annualized Sharpe ratio from daily log returns.
fn compute_sharpe(portfolio_returns: &[f64]) -> f64 {
    let n = portfolio_returns.len() as f64;
    let daily_rf = (1.0 + ANNUAL_RISK_FREE_RATE).powf(1.0 / TRADING_DAYS_PER_YEAR) - 1.0;

    let excess_returns: Vec<f64> = portfolio_returns.iter().map(|r| r - daily_rf).collect();
    let mean_excess: f64 = excess_returns.iter().sum::<f64>() / n;
    let var: f64 = excess_returns
        .iter()
        .map(|r| (r - mean_excess).powi(2))
        .sum::<f64>()
        / (n - 1.0);
    let std = var.sqrt();

    if std > 0.0 {
        (mean_excess / std) * TRADING_DAYS_PER_YEAR.sqrt()
    } else {
        0.0
    }
}

/// Computes beta = cov(portfolio, benchmark) / var(benchmark) from daily log returns.
fn compute_beta(portfolio_returns: &[f64], benchmark_returns: &[f64]) -> f64 {
    let n = portfolio_returns.len() as f64;
    let mean_port: f64 = portfolio_returns.iter().sum::<f64>() / n;
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

    if var_bench > 0.0 {
        cov / var_bench
    } else {
        0.0
    }
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
        currency: BENCHMARK_CURRENCY.to_owned(),
    };

    let asset_id = asset_repo::get_or_create(db, &info).await?;

    let asset = crate::models::Asset {
        id: asset_id,
        ticker: BENCHMARK_TICKER.to_owned(),
        isin: None,
        name: BENCHMARK_NAME.to_owned(),
        asset_type: AssetType::Stock,
        currency: BENCHMARK_CURRENCY.to_owned(),
    };

    daily_prices::fill_prices_for_range(db, &asset, start_date, end_date, price_fetcher).await?;

    Ok(asset_id)
}
