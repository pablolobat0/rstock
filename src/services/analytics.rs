use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::constants::{BASE_CURRENCY, MIN_DATA_POINTS, ZERO_RETURN_THRESHOLD};
use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
};
use crate::models::{
    Asset, AssetClassification, AssetType, CorrelationMatrix, PeriodMetrics, PortfolioSnapshot,
    RollingCorrelationResult,
};
use crate::services::price::PriceFetcher;
use crate::services::{historical_market_data, metrics};

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

    let benchmark = get_or_create_benchmark_asset(db).await?;
    historical_market_data::prepare_benchmark_market_data(
        db,
        &benchmark,
        start_date,
        end_date,
        price_fetcher,
    )
    .await?;

    let mut all_assets: Vec<Asset> = assets;
    all_assets.push(benchmark);

    let mut return_series: Vec<(String, HashMap<String, f64>)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for asset in &all_assets {
        let eur_prices =
            historical_market_data::get_base_currency_price_series(db, asset, start_date, end_date)
                .await?;

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

pub async fn compute_rolling_correlation_data(
    _db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    ticker_a: &str,
    ticker_b: &str,
    period_label: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<RollingCorrelationResult> {
    if ticker_a == ticker_b {
        anyhow::bail!("tickers must be different");
    }

    let left_asset = fetch_rolling_asset_info(ticker_a, price_fetcher).await?;
    let right_asset = fetch_rolling_asset_info(ticker_b, price_fetcher).await?;

    let left_prices = historical_market_data::fetch_direct_base_currency_price_series(
        &left_asset,
        start_date,
        end_date,
        price_fetcher,
    )
    .await?;
    let right_prices = historical_market_data::fetch_direct_base_currency_price_series(
        &right_asset,
        start_date,
        end_date,
        price_fetcher,
    )
    .await?;

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

    let benchmark = get_or_create_benchmark_asset(db).await?;
    historical_market_data::prepare_benchmark_market_data(
        db,
        &benchmark,
        widest_start,
        snapshot_date,
        price_fetcher,
    )
    .await?;
    let benchmark_prices =
        daily_price_repo::find_prices_between(db, benchmark.id, widest_start, snapshot_date)
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
        let sortino = metrics::compute_sortino(&portfolio_returns);

        results.push(Some(PeriodMetrics {
            volatility,
            max_drawdown,
            beta,
            sharpe,
            sortino,
        }));
    }

    Ok((
        results[0].take(),
        results[1].take(),
        results[2].take(),
        results[3].take(),
    ))
}

async fn get_or_create_benchmark_asset(db: &DatabaseConnection) -> anyhow::Result<Asset> {
    let info = metrics::benchmark_asset_info();
    if let Some(asset) = asset_repo::find_by_ticker(db, &info.ticker).await? {
        return Ok(asset);
    }

    let id = asset_repo::create(db, &info, &AssetClassification::default(), None).await?;
    Ok(metrics::benchmark_asset(id))
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

async fn fetch_rolling_asset_info(
    ticker: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Asset> {
    let stock_info = price_fetcher.get_stock_info(ticker).await?;
    let name = stock_info.name.unwrap_or_else(|| ticker.to_owned());
    let currency = stock_info
        .currency
        .unwrap_or_else(|| BASE_CURRENCY.to_owned());

    Ok(Asset {
        id: 0,
        ticker: ticker.to_owned(),
        name,
        asset_type: AssetType::Stock,
        currency,
        morningstar_code: None,
        asset_class: None,
        equity_style: None,
        management: None,
    })
}
