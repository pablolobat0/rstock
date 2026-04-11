use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::constants::{BASE_CURRENCY, BENCHMARK_CURRENCY, MIN_DATA_POINTS, ZERO_RETURN_THRESHOLD};
use crate::db::repos::{
    asset_repo, daily_price_repo, exchange_rate_repo, portfolio_asset_history_repo,
    portfolio_history_repo,
};
use crate::models::{Asset, CorrelationMatrix, PeriodMetrics, PortfolioSnapshot};
use crate::services::price::PriceFetcher;
use crate::services::{daily_prices, exchange_rates, metrics, price_cache};

pub async fn compute_correlation_data(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<CorrelationMatrix> {
    let latest = portfolio_history_repo::find_latest(db).await?;
    let held_assets = match &latest {
        Some(snap) => portfolio_asset_history_repo::find_by_date(db, &snap.date).await?,
        None => vec![],
    };

    let asset_ids: Vec<i32> = held_assets.iter().map(|s| s.asset_id).collect();
    let assets = asset_repo::find_by_ids(db, asset_ids.into_iter()).await?;

    let benchmark_asset_id =
        ensure_benchmark_prices(db, start_date, end_date, price_fetcher).await?;
    if BENCHMARK_CURRENCY != BASE_CURRENCY {
        let pair = exchange_rates::currency_pair(BENCHMARK_CURRENCY);
        price_cache::fill_exchange_rates(db, &[pair], start_date, end_date, price_fetcher).await?;
    }

    let mut all_assets: Vec<Asset> = assets;
    all_assets.push(metrics::benchmark_asset(benchmark_asset_id));

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

        let returns = metrics::compute_log_returns(&eur_prices);
        if returns.len() < MIN_DATA_POINTS {
            warnings.push(asset.name.clone());
        }
        return_series.push((asset.name.clone(), returns));
    }

    let n = return_series.len();
    let names: Vec<String> = return_series.iter().map(|(name, _)| name.clone()).collect();
    let mut matrix = vec![vec![None; n]; n];

    for i in 0..n {
        matrix[i][i] = Some(1.0);
        for j in (i + 1)..n {
            let (aligned_a, aligned_b) =
                metrics::align_return_series(&return_series[i].1, &return_series[j].1);
            if aligned_a.len() >= MIN_DATA_POINTS {
                let corr = metrics::pearson_correlation(&aligned_a, &aligned_b);
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

#[allow(clippy::too_many_arguments)]
pub async fn compute_all_period_metrics(
    db: &DatabaseConnection,
    snapshot_date: &str,
    ytd_date: &str,
    one_year_date: &str,
    three_year_date: &str,
    five_year_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<(
    Option<PeriodMetrics>,
    Option<PeriodMetrics>,
    Option<PeriodMetrics>,
    Option<PeriodMetrics>,
)> {
    let widest_start = five_year_date
        .min(three_year_date)
        .min(one_year_date)
        .min(ytd_date);

    let all_snapshots =
        portfolio_history_repo::find_between(db, widest_start, snapshot_date).await?;
    if all_snapshots.len() < 2 {
        return Ok((None, None, None, None));
    }

    let benchmark_asset_id =
        ensure_benchmark_prices(db, widest_start, snapshot_date, price_fetcher).await?;
    let benchmark_prices =
        daily_price_repo::find_prices_between(db, benchmark_asset_id, widest_start, snapshot_date)
            .await?;
    let benchmark_map: HashMap<&str, f64> = benchmark_prices
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

        results.push(Some(PeriodMetrics {
            volatility,
            max_drawdown,
            beta,
            sharpe,
        }));
    }

    Ok((
        results[0].take(),
        results[1].take(),
        results[2].take(),
        results[3].take(),
    ))
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

async fn ensure_benchmark_prices(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<i32> {
    let asset_id = asset_repo::get_or_create(db, &metrics::benchmark_asset_info()).await?;
    let asset = metrics::benchmark_asset(asset_id);

    daily_prices::fill_prices_for_range(db, &asset, start_date, end_date, price_fetcher).await?;

    Ok(asset_id)
}
