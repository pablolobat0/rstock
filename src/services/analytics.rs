use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::constants::{
    is_benchmark_ticker, FLOAT_EPSILON, MIN_DATA_POINTS, ZERO_RETURN_THRESHOLD,
};
use crate::db::repos::{asset_repo, portfolio_history_repo, transaction_repo};
use crate::models::{
    Asset, CorrelationMatrix, MarketDataLimitation, PeriodMetrics, PortfolioSnapshot,
    RollingCorrelationResult,
};
use crate::services::market_data::MarketData;
use crate::services::metrics;

pub struct PeriodMetricsResult {
    pub ytd: Option<PeriodMetrics>,
    pub one_year: Option<PeriodMetrics>,
    pub three_year: Option<PeriodMetrics>,
    pub five_year: Option<PeriodMetrics>,
    pub market_data_limitations: Vec<MarketDataLimitation>,
}

pub async fn compute_correlation_data(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<CorrelationMatrix> {
    let assets = current_correlation_assets(db, end_date).await?;

    let correlation_market_data = market_data
        .correlation_market_data(db, assets, start_date, end_date)
        .await?;

    let mut return_series: Vec<(String, HashMap<String, f64>)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for series in correlation_market_data
        .tracked_asset_series
        .iter()
        .chain(std::iter::once(&correlation_market_data.benchmark_series))
    {
        let returns = metrics::compute_log_returns(&series.prices);
        if returns.len() < MIN_DATA_POINTS {
            warnings.push(series.name.clone());
        }
        return_series.push((series.name.clone(), returns));
    }

    let n = return_series.len();
    let names: Vec<String> = return_series.iter().map(|(name, _)| name.clone()).collect();
    let mut matrix = vec![vec![None; n]; n];

    for i in 0..n {
        matrix[i][i] = Some(1.0);
        for j in (i + 1)..n {
            if let Some(corr) = compute_return_correlation(&return_series[i].1, &return_series[j].1)
            {
                matrix[i][j] = Some(corr);
                matrix[j][i] = Some(corr);
            }
        }
    }

    Ok(CorrelationMatrix {
        names,
        matrix,
        warnings,
        market_data_limitations: correlation_market_data.limitations,
    })
}

async fn current_correlation_assets(
    db: &DatabaseConnection,
    end_date: &str,
) -> anyhow::Result<Vec<Asset>> {
    let transactions = transaction_repo::find_all_ordered_by_date(db, None, Some(end_date)).await?;
    let mut transactions_by_asset: HashMap<i32, Vec<crate::models::Transaction>> = HashMap::new();
    for transaction in transactions {
        transactions_by_asset
            .entry(transaction.asset_id)
            .or_default()
            .push(transaction);
    }

    let assets = asset_repo::find_by_ids(db, transactions_by_asset.keys().copied()).await?;
    Ok(assets
        .into_iter()
        .filter(|asset| !asset.is_monetary() && !is_benchmark_ticker(&asset.ticker))
        .filter(|asset| {
            transactions_by_asset
                .get(&asset.id)
                .is_some_and(|transactions| {
                    crate::models::Transaction::compute_holdings(transactions) > FLOAT_EPSILON
                })
        })
        .collect())
}

#[allow(clippy::implicit_hasher)]
pub fn compute_return_correlation(
    left_returns: &HashMap<String, f64>,
    right_returns: &HashMap<String, f64>,
) -> Option<f64> {
    let (aligned_left, aligned_right) = metrics::align_return_series(left_returns, right_returns);
    if aligned_left.len() < MIN_DATA_POINTS {
        return None;
    }
    Some(metrics::pearson_correlation(&aligned_left, &aligned_right))
}

pub async fn compute_rolling_correlation_data(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    identifier_a: &str,
    identifier_b: &str,
    period_label: &str,
    market_data: &MarketData,
) -> anyhow::Result<RollingCorrelationResult> {
    if identifier_a == identifier_b {
        anyhow::bail!("tracked asset identifiers must be different");
    }

    let left_asset = find_tracked_asset(db, identifier_a).await?;
    let right_asset = find_tracked_asset(db, identifier_b).await?;

    let (series, market_data_limitations) = market_data
        .tracked_correlation_market_data(
            db,
            vec![left_asset.clone(), right_asset.clone()],
            start_date,
            end_date,
        )
        .await?;
    let left_prices = series_prices(&series, left_asset.id)?;
    let right_prices = series_prices(&series, right_asset.id)?;

    let left_returns = metrics::compute_log_returns(&left_prices);
    let right_returns = metrics::compute_log_returns(&right_prices);
    let aligned = metrics::align_return_series_with_dates_unfiltered(&left_returns, &right_returns);
    let points = metrics::compute_rolling_correlation(&aligned);
    let (latest, min, max, average) = metrics::summarize_rolling_correlation(&points);

    Ok(RollingCorrelationResult {
        left_name: left_asset.name,
        right_name: right_asset.name,
        period_label: period_label.to_owned(),
        window_label: format!(
            "{}D rolling",
            crate::constants::ROLLING_CORRELATION_WINDOW_DAYS
        ),
        requested_start_date: start_date.to_owned(),
        requested_end_date: end_date.to_owned(),
        points,
        latest,
        min,
        max,
        average,
        market_data_limitations,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn compute_all_period_metrics(
    db: &DatabaseConnection,
    snapshot_date: &str,
    ytd_date: &str,
    one_year_date: &str,
    three_year_date: &str,
    five_year_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<PeriodMetricsResult> {
    let widest_start = five_year_date
        .min(three_year_date)
        .min(one_year_date)
        .min(ytd_date);

    let all_snapshots =
        portfolio_history_repo::find_between(db, widest_start, snapshot_date).await?;
    if all_snapshots.len() < 2 {
        return Ok(PeriodMetricsResult {
            ytd: None,
            one_year: None,
            three_year: None,
            five_year: None,
            market_data_limitations: Vec::new(),
        });
    }

    let benchmark_market_data = market_data
        .correlation_market_data(db, Vec::new(), widest_start, snapshot_date)
        .await?;
    let benchmark_map: HashMap<&str, f64> = benchmark_market_data
        .benchmark_series
        .prices
        .iter()
        .map(|(d, p)| (d.as_str(), *p))
        .collect();

    let period_dates = [ytd_date, one_year_date, three_year_date, five_year_date];
    let mut results = Vec::new();

    for start_date in &period_dates {
        if portfolio_history_repo::find_at_or_before(db, start_date)
            .await?
            .is_none()
        {
            results.push(None);
            continue;
        }

        let period_snapshots: Vec<&PortfolioSnapshot> = all_snapshots
            .iter()
            .filter(|s| s.date.as_str() >= *start_date)
            .collect();

        if period_snapshots.len() < 2 {
            results.push(None);
            continue;
        }

        let (portfolio_returns, benchmark_returns, trading_day_navs) =
            filter_trading_days(&period_snapshots, &benchmark_map);

        let volatility = metrics::compute_volatility(&portfolio_returns);
        let max_drawdown = metrics::compute_max_drawdown(&trading_day_navs);
        let beta = metrics::compute_beta(&portfolio_returns, &benchmark_returns);
        let sharpe = metrics::compute_sharpe(&portfolio_returns);
        let sortino = metrics::compute_sortino(&portfolio_returns);

        results.push(Some(PeriodMetrics {
            volatility,
            max_drawdown,
            beta,
            sharpe,
            sortino,
        }));
    }

    Ok(PeriodMetricsResult {
        ytd: results[0].take(),
        one_year: results[1].take(),
        three_year: results[2].take(),
        five_year: results[3].take(),
        market_data_limitations: benchmark_market_data.limitations,
    })
}

fn series_prices(
    series: &[crate::models::CorrelationMarketDataSeries],
    asset_id: i32,
) -> anyhow::Result<crate::models::BaseCurrencyPriceSeries> {
    series
        .iter()
        .find(|series| series.asset_id == asset_id)
        .map(|series| series.prices.clone())
        .ok_or_else(|| anyhow::anyhow!("missing correlation market data for asset {asset_id}"))
}

fn filter_trading_days(
    snapshots: &[&PortfolioSnapshot],
    benchmark_map: &HashMap<&str, f64>,
) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
    let mut portfolio_returns = Vec::new();
    let mut benchmark_returns = Vec::new();
    let mut trading_day_navs = Vec::new();

    let mut first_added = false;

    for i in 1..snapshots.len() {
        let date = snapshots[i].date.as_str();
        let prev_date = snapshots[i - 1].date.as_str();

        if let (Some(&bench_today), Some(&bench_prev)) =
            (benchmark_map.get(date), benchmark_map.get(prev_date))
        {
            if bench_prev > 0.0 && snapshots[i - 1].nav > 0.0 {
                let bench_ret = (bench_today / bench_prev).ln();
                if bench_ret.abs() < ZERO_RETURN_THRESHOLD {
                    continue;
                }
                let port_ret = (snapshots[i].nav / snapshots[i - 1].nav).ln();
                portfolio_returns.push(port_ret);
                benchmark_returns.push(bench_ret);

                if !first_added {
                    trading_day_navs.push(snapshots[i - 1].nav);
                    first_added = true;
                }
                trading_day_navs.push(snapshots[i].nav);
            }
        }
    }

    (portfolio_returns, benchmark_returns, trading_day_navs)
}

async fn find_tracked_asset(db: &DatabaseConnection, identifier: &str) -> anyhow::Result<Asset> {
    asset_repo::find_by_ticker(db, identifier)
        .await?
        .ok_or_else(|| anyhow::anyhow!("tracked asset '{identifier}' not found"))
}
