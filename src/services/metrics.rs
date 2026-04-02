use sea_orm::DatabaseConnection;
use std::collections::HashMap;

use crate::constants::{
    ANNUAL_RISK_FREE_RATE, BASE_CURRENCY, BENCHMARK_CURRENCY, BENCHMARK_NAME, BENCHMARK_TICKER,
    MIN_DATA_POINTS, TRADING_DAYS_PER_YEAR, ZERO_RETURN_THRESHOLD,
};
use crate::db::repos::{
    asset_repo, daily_price_repo, exchange_rate_repo, portfolio_asset_history_repo,
    portfolio_history_repo,
};
use crate::models::{
    Asset, AssetInfo, AssetType, CorrelationMatrix, PeriodMetrics, PortfolioSnapshot,
};
use crate::services::daily_prices;
use crate::services::exchange_rates;
use crate::services::portfolio;
use crate::services::price::PriceFetcher;
use crate::services::price_cache;

pub fn is_benchmark_ticker(ticker: &str) -> bool {
    ticker == BENCHMARK_TICKER
}

/// Computes annualized volatility from a slice of daily NAV snapshots.
/// Returns `None` if fewer than 2 snapshots.
pub fn compute_volatility(snapshots: &[PortfolioSnapshot]) -> Option<f64> {
    if snapshots.len() < 2 {
        return None;
    }

    let returns: Vec<f64> = snapshots
        .windows(2)
        .filter_map(|w| {
            if w[0].nav > 0.0 {
                Some((w[1].nav / w[0].nav).ln())
            } else {
                None
            }
        })
        .collect();

    if returns.len() < 2 {
        return None;
    }

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let var = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std = var.sqrt();

    Some(std * TRADING_DAYS_PER_YEAR.sqrt() * 100.0)
}

/// Computes max drawdown from a slice of daily NAV snapshots.
/// Returns the worst peak-to-trough decline as a negative percentage.
/// Returns `None` if fewer than 2 snapshots.
pub fn compute_max_drawdown(snapshots: &[PortfolioSnapshot]) -> Option<f64> {
    if snapshots.len() < 2 {
        return None;
    }

    let mut peak = snapshots[0].nav;
    let mut max_dd = 0.0_f64;

    for snap in &snapshots[1..] {
        if snap.nav > peak {
            peak = snap.nav;
        }
        if peak > 0.0 {
            let dd = (snap.nav - peak) / peak * 100.0;
            if dd < max_dd {
                max_dd = dd;
            }
        }
    }

    Some(max_dd)
}

/// Computes beta and Sharpe ratio for a given date range.
/// Returns `None` if insufficient aligned data points.
pub async fn compute_beta_sharpe_for_period(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<(f64, f64)>> {
    let benchmark_asset_id =
        ensure_benchmark_prices(db, start_date, end_date, price_fetcher).await?;

    let nav_snapshots = portfolio_history_repo::find_between(db, start_date, end_date).await?;
    if nav_snapshots.len() < MIN_DATA_POINTS {
        return Ok(None);
    }

    let benchmark_prices =
        daily_price_repo::find_prices_between(db, benchmark_asset_id, start_date, end_date).await?;

    let benchmark_map: HashMap<&str, f64> = benchmark_prices
        .iter()
        .map(|(d, p)| (d.as_str(), *p))
        .collect();

    let (portfolio_returns, benchmark_returns) = align_returns(&nav_snapshots, &benchmark_map);

    if portfolio_returns.len() < MIN_DATA_POINTS {
        return Ok(None);
    }

    let sharpe = compute_sharpe(&portfolio_returns);
    let beta = compute_beta(&portfolio_returns, &benchmark_returns);

    Ok(Some((beta, sharpe)))
}

/// Computes all period metrics (volatility, max drawdown, beta, Sharpe) for a date range.
/// Returns `None` if the portfolio didn't exist at the start of the period.
pub async fn compute_period_metrics(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<PeriodMetrics>> {
    // If no snapshot exists at or before start_date, the period predates the portfolio
    if portfolio_history_repo::find_at_or_before(db, start_date)
        .await?
        .is_none()
    {
        return Ok(None);
    }

    let snapshots = portfolio_history_repo::find_between(db, start_date, end_date).await?;
    if snapshots.len() < 2 {
        return Ok(None);
    }

    let volatility = compute_volatility(&snapshots);
    let max_drawdown = compute_max_drawdown(&snapshots);

    let (beta, sharpe) =
        match compute_beta_sharpe_for_period(db, start_date, end_date, price_fetcher).await? {
            Some((b, s)) => (Some(b), Some(s)),
            None => (None, None),
        };

    Ok(Some(PeriodMetrics {
        volatility,
        max_drawdown,
        beta,
        sharpe,
    }))
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

/// Computes correlation matrix between all held assets and the reference index.
pub async fn compute_correlation_matrix(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<CorrelationMatrix> {
    // 0. Ensure portfolio history is up to date
    portfolio::trigger_rebuild_if_needed(db, price_fetcher).await?;

    // 1. Get held assets from latest portfolio snapshot
    let latest = portfolio_history_repo::find_latest(db).await?;
    let held_assets = match &latest {
        Some(snap) => portfolio_asset_history_repo::find_by_date(db, &snap.date).await?,
        None => vec![],
    };

    let asset_ids: Vec<i32> = held_assets.iter().map(|s| s.asset_id).collect();
    let assets = asset_repo::find_by_ids(db, asset_ids.into_iter()).await?;

    // 2. Ensure benchmark prices and FX rate are cached
    let benchmark_asset_id =
        ensure_benchmark_prices(db, start_date, end_date, price_fetcher).await?;
    if BENCHMARK_CURRENCY != BASE_CURRENCY {
        let pair = exchange_rates::currency_pair(BENCHMARK_CURRENCY);
        price_cache::fill_exchange_rates(db, &[pair], start_date, end_date, price_fetcher).await?;
    }

    // 3. Build list: held assets + benchmark
    let mut all_assets: Vec<Asset> = assets;
    all_assets.push(benchmark_asset(benchmark_asset_id));

    // 4. For each asset: fetch cached prices + FX, convert to EUR returns
    let mut return_series: Vec<(String, HashMap<String, f64>)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for asset in &all_assets {
        let fx_pair = if asset.currency == BASE_CURRENCY {
            None
        } else {
            Some(exchange_rates::currency_pair(&asset.currency))
        };

        let prices =
            daily_price_repo::find_prices_between(db, asset.id, start_date, end_date).await?;

        let eur_prices = if let Some(ref pair) = fx_pair {
            let rates =
                exchange_rate_repo::find_rates_between(db, pair, start_date, end_date).await?;
            let rate_map: HashMap<&str, f64> =
                rates.iter().map(|(d, r)| (d.as_str(), *r)).collect();

            prices
                .iter()
                .filter_map(|(date, price)| {
                    rate_map
                        .get(date.as_str())
                        .map(|rate| (date.clone(), price * rate))
                })
                .collect::<Vec<_>>()
        } else {
            prices
        };

        let returns = compute_log_returns(&eur_prices);
        if returns.len() < MIN_DATA_POINTS {
            warnings.push(asset.name.clone());
        }
        return_series.push((asset.name.clone(), returns));
    }

    // 5. Build N×N correlation matrix
    let n = return_series.len();
    let names: Vec<String> = return_series.iter().map(|(name, _)| name.clone()).collect();
    let mut matrix = vec![vec![None; n]; n];

    for i in 0..n {
        matrix[i][i] = Some(1.0); // diagonal
        for j in (i + 1)..n {
            let (aligned_a, aligned_b) =
                align_return_series(&return_series[i].1, &return_series[j].1);
            if aligned_a.len() >= MIN_DATA_POINTS {
                let corr = pearson_correlation(&aligned_a, &aligned_b);
                matrix[i][j] = Some(corr);
                matrix[j][i] = Some(corr);
            }
        }
    }

    Ok(CorrelationMatrix {
        names,
        matrix,
        warnings,
    })
}

/// Computes Pearson correlation coefficient between two aligned return series.
fn pearson_correlation(x: &[f64], y: &[f64]) -> f64 {
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
fn compute_log_returns(prices: &[(String, f64)]) -> HashMap<String, f64> {
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
fn align_return_series(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> (Vec<f64>, Vec<f64>) {
    let mut aligned_a = Vec::new();
    let mut aligned_b = Vec::new();

    for (date, &ret_a) in a {
        if let Some(&ret_b) = b.get(date) {
            // Skip days where both returns are essentially zero (forward-filled)
            if ret_a.abs() < ZERO_RETURN_THRESHOLD && ret_b.abs() < ZERO_RETURN_THRESHOLD {
                continue;
            }
            aligned_a.push(ret_a);
            aligned_b.push(ret_b);
        }
    }

    (aligned_a, aligned_b)
}

fn benchmark_asset_info() -> AssetInfo {
    AssetInfo {
        ticker: BENCHMARK_TICKER.to_owned(),
        name: BENCHMARK_NAME.to_owned(),
        asset_type: AssetType::Stock,
        currency: BENCHMARK_CURRENCY.to_owned(),
    }
}

fn benchmark_asset(id: i32) -> Asset {
    Asset {
        id,
        ticker: BENCHMARK_TICKER.to_owned(),
        name: BENCHMARK_NAME.to_owned(),
        asset_type: AssetType::Stock,
        currency: BENCHMARK_CURRENCY.to_owned(),
    }
}

async fn ensure_benchmark_prices(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<i32> {
    let asset_id = asset_repo::get_or_create(db, &benchmark_asset_info()).await?;
    let asset = benchmark_asset(asset_id);

    daily_prices::fill_prices_for_range(db, &asset, start_date, end_date, price_fetcher).await?;

    Ok(asset_id)
}
