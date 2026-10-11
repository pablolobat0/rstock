use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::constants::{is_benchmark_ticker, FLOAT_EPSILON, MIN_DATA_POINTS};
use crate::db::repos::{asset_repo, transaction_repo};
use crate::models::{Asset, CorrelationMatrix, RollingCorrelationResult};
use crate::services::market_data::MarketData;
use crate::services::{ledger, metrics};

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
    let mut current_assets = Vec::new();
    for asset in assets {
        if asset.is_monetary() || is_benchmark_ticker(&asset.ticker) {
            continue;
        }
        let Some(transactions) = transactions_by_asset.get(&asset.id) else {
            continue;
        };
        let replay = ledger::replay_transactions(asset.id, transactions)
            .map_err(|error| anyhow::anyhow!(error))?;
        if replay.final_quantity > FLOAT_EPSILON {
            current_assets.push(asset);
        }
    }
    Ok(current_assets)
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

async fn find_tracked_asset(db: &DatabaseConnection, identifier: &str) -> anyhow::Result<Asset> {
    asset_repo::find_by_ticker(db, identifier)
        .await?
        .ok_or_else(|| anyhow::anyhow!("tracked asset '{identifier}' not found"))
}
