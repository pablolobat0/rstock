use std::collections::HashMap;

use crate::constants::{
    ANNUAL_RISK_FREE_RATE, BENCHMARK_CURRENCY, BENCHMARK_NAME, BENCHMARK_TICKER, MIN_DATA_POINTS,
    TRADING_DAYS_PER_YEAR, ZERO_RETURN_THRESHOLD,
};
use crate::models::{Asset, AssetInfo, AssetType};

/// Computes annualized volatility from pre-filtered daily log returns (trading days only).
/// Returns `None` if fewer than 2 returns.
pub fn compute_volatility(daily_returns: &[f64]) -> Option<f64> {
    if daily_returns.len() < 2 {
        return None;
    }

    let n = daily_returns.len() as f64;
    let mean = daily_returns.iter().sum::<f64>() / n;
    let var = daily_returns
        .iter()
        .map(|r| (r - mean).powi(2))
        .sum::<f64>()
        / (n - 1.0);
    let std = var.sqrt();

    Some(std * TRADING_DAYS_PER_YEAR.sqrt() * 100.0)
}

/// Computes max drawdown from a sequence of NAV values (trading days only).
/// Returns the worst peak-to-trough decline as a negative percentage.
/// Returns `None` if fewer than 2 values.
pub fn compute_max_drawdown(nav_values: &[f64]) -> Option<f64> {
    if nav_values.len() < 2 {
        return None;
    }

    let mut peak = nav_values[0];
    let mut max_dd = 0.0_f64;

    for &nav in &nav_values[1..] {
        if nav > peak {
            peak = nav;
        }
        if peak > 0.0 {
            let dd = (nav - peak) / peak * 100.0;
            if dd < max_dd {
                max_dd = dd;
            }
        }
    }

    Some(max_dd)
}

/// Computes annualized Sharpe ratio from daily log returns.
/// Returns `None` if fewer than `MIN_DATA_POINTS` returns.
pub fn compute_sharpe(daily_returns: &[f64]) -> Option<f64> {
    if daily_returns.len() < MIN_DATA_POINTS {
        return None;
    }

    let n = daily_returns.len() as f64;
    let daily_rf = (1.0 + ANNUAL_RISK_FREE_RATE).powf(1.0 / TRADING_DAYS_PER_YEAR) - 1.0;

    let excess_returns: Vec<f64> = daily_returns.iter().map(|r| r - daily_rf).collect();
    let mean_excess: f64 = excess_returns.iter().sum::<f64>() / n;
    let var: f64 = excess_returns
        .iter()
        .map(|r| (r - mean_excess).powi(2))
        .sum::<f64>()
        / (n - 1.0);
    let std = var.sqrt();

    if std > 0.0 {
        Some((mean_excess / std) * TRADING_DAYS_PER_YEAR.sqrt())
    } else {
        Some(0.0)
    }
}

/// Computes beta = cov(portfolio, benchmark) / var(benchmark) from daily log returns.
/// Returns `None` if fewer than `MIN_DATA_POINTS` returns.
pub fn compute_beta(portfolio_returns: &[f64], benchmark_returns: &[f64]) -> Option<f64> {
    if portfolio_returns.len() < MIN_DATA_POINTS || benchmark_returns.len() < MIN_DATA_POINTS {
        return None;
    }

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
        Some(cov / var_bench)
    } else {
        Some(0.0)
    }
}

/// Computes Pearson correlation coefficient between two aligned return series.
pub fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;

    let cov: f64 = x
        .iter()
        .zip(y.iter())
        .map(|(xi, yi)| (xi - mean_x) * (yi - mean_y))
        .sum::<f64>()
        / (n - 1.0);

    let std_x = (x.iter().map(|xi| (xi - mean_x).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
    let std_y = (y.iter().map(|yi| (yi - mean_y).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();

    if std_x > 0.0 && std_y > 0.0 {
        cov / (std_x * std_y)
    } else {
        0.0
    }
}

/// Computes daily log returns from a sorted price series.
pub fn compute_log_returns(prices: &[(String, f64)]) -> HashMap<String, f64> {
    let mut returns = HashMap::new();
    for window in prices.windows(2) {
        if window[0].1 > 0.0 {
            let ret = (window[1].1 / window[0].1).ln();
            returns.insert(window[1].0.clone(), ret);
        }
    }
    returns
}

/// Aligns two return series by date intersection, filtering out zero-return days.
#[allow(clippy::implicit_hasher)]
pub fn align_return_series(
    a: &HashMap<String, f64>,
    b: &HashMap<String, f64>,
) -> (Vec<f64>, Vec<f64>) {
    let mut aligned_a = Vec::new();
    let mut aligned_b = Vec::new();

    for (date, &ret_a) in a {
        if let Some(&ret_b) = b.get(date) {
            if ret_a.abs() < ZERO_RETURN_THRESHOLD && ret_b.abs() < ZERO_RETURN_THRESHOLD {
                continue;
            }
            aligned_a.push(ret_a);
            aligned_b.push(ret_b);
        }
    }

    (aligned_a, aligned_b)
}

pub fn benchmark_asset_info() -> AssetInfo {
    AssetInfo {
        ticker: BENCHMARK_TICKER.to_owned(),
        name: BENCHMARK_NAME.to_owned(),
        asset_type: AssetType::Stock,
        currency: BENCHMARK_CURRENCY.to_owned(),
    }
}

pub fn benchmark_asset(id: i32) -> Asset {
    Asset {
        id,
        ticker: BENCHMARK_TICKER.to_owned(),
        name: BENCHMARK_NAME.to_owned(),
        asset_type: AssetType::Stock,
        currency: BENCHMARK_CURRENCY.to_owned(),
        morningstar_code: None,
        asset_class: None,
        equity_style: None,
        management: None,
    }
}
