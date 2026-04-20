use std::collections::HashMap;

use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, BENCHMARK_TICKER, DATE_FORMAT, FIVE_YEAR_TRADING_DAYS, ONE_YEAR_TRADING_DAYS,
    THREE_YEAR_TRADING_DAYS,
};
use crate::db::repos::{asset_repo, fund_holdings_snapshot_repo};
use crate::models::{
    AllocationEntry, AssetType, FundAnalysisResult, FundData, FundHolding, FundPeriodMetrics,
    HoldingChange, HoldingChangeType,
};
use crate::services::metrics;
use crate::services::price::PriceFetcher;
use crate::utils::resolve_scripts_dir;

pub async fn compute_fund_analysis(
    db: &DatabaseConnection,
    fetcher: &dyn PriceFetcher,
    ms_code: &str,
) -> anyhow::Result<FundAnalysisResult> {
    let today = chrono::Local::now().date_naive();
    let today_str = format_date(today);

    let asset = asset_repo::find_by_morningstar_code(db, ms_code).await?;
    let name = asset.as_ref().map(|a| a.name.clone());

    let (fund_data_result, prices_result) = tokio::join!(
        fetch_fund_data(ms_code, 200),
        fetcher.get_historical_prices(ms_code, "2000-01-01", &today_str, &AssetType::Fund),
    );

    let fund_data = fund_data_result?;
    let fund_prices = prices_result?;

    let fund_currency = fund_data.fund_currency.clone();
    let total_holdings = fund_data.total_holdings;
    let portfolio_date = fund_data.portfolio_date.clone();
    let all_holdings = fund_data.holdings;
    let equity_holdings: Vec<FundHolding> = all_holdings
        .iter()
        .filter(|holding| is_equity_holding(holding))
        .cloned()
        .collect();

    let earliest_date = fund_prices
        .first()
        .map_or("2000-01-01", |(d, _)| d.as_str());
    let benchmark_prices = fetcher
        .get_historical_prices(
            BENCHMARK_TICKER,
            earliest_date,
            &today_str,
            &AssetType::Stock,
        )
        .await
        .unwrap_or_default();

    let benchmark_returns = metrics::compute_log_returns(&benchmark_prices);

    let ytd_start = NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 always valid");
    let ytd = compute_period_metrics(&fund_prices, &benchmark_returns, ytd_start, today);
    let one_year =
        compute_trailing_period_metrics(&fund_prices, &benchmark_returns, ONE_YEAR_TRADING_DAYS);
    let three_year =
        compute_trailing_period_metrics(&fund_prices, &benchmark_returns, THREE_YEAR_TRADING_DAYS);
    let five_year =
        compute_trailing_period_metrics(&fund_prices, &benchmark_returns, FIVE_YEAR_TRADING_DAYS);
    let all_time = if fund_prices.len() >= 2 {
        let start = NaiveDate::parse_from_str(&fund_prices[0].0, DATE_FORMAT).ok();
        start.and_then(|s| compute_period_metrics(&fund_prices, &benchmark_returns, s, today))
    } else {
        None
    };

    let sector_breakdown = compute_breakdown(&equity_holdings, |h| h.sector.clone());
    let country_breakdown = compute_breakdown(&equity_holdings, |h| h.country.clone());
    let currency_breakdown = compute_breakdown(&equity_holdings, |h| h.currency.clone());

    let (holdings_changed, last_snapshot_date, holding_diff) = compute_snapshot_diff(
        db,
        ms_code,
        portfolio_date.as_deref(),
        &all_holdings,
        total_holdings,
        &today_str,
    )
    .await?;

    let top_holdings: Vec<FundHolding> = all_holdings.into_iter().take(30).collect();

    Ok(FundAnalysisResult {
        ms_code: ms_code.to_owned(),
        name,
        fund_currency,
        total_holdings,
        portfolio_date,
        top_holdings,
        sector_breakdown,
        country_breakdown,
        currency_breakdown,
        ytd,
        one_year,
        three_year,
        five_year,
        all_time,
        holdings_changed,
        last_snapshot_date,
        holding_diff,
    })
}

pub fn compute_fingerprint(holdings: &[FundHolding]) -> String {
    let mut entries: Vec<(&str, f64)> = holdings
        .iter()
        .map(|h| (h.name.as_str(), h.weighting))
        .collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let compact: Vec<serde_json::Value> = entries
        .iter()
        .map(|(name, weight)| serde_json::json!([name, weight]))
        .collect();
    serde_json::to_string(&compact).unwrap_or_default()
}

pub fn compute_holding_diff(
    old_json: &str,
    new_holdings: &[FundHolding],
) -> anyhow::Result<Vec<HoldingChange>> {
    let old_entries: Vec<serde_json::Value> =
        serde_json::from_str(old_json).context("failed to parse old holdings JSON")?;

    let old_map: HashMap<String, f64> = old_entries
        .iter()
        .filter_map(|e| {
            let name = e["name"].as_str()?.to_owned();
            let weight = e["weighting"].as_f64()?;
            Some((name, weight))
        })
        .collect();

    let new_map: HashMap<&str, f64> = new_holdings
        .iter()
        .map(|h| (h.name.as_str(), h.weighting))
        .collect();

    let mut changes = Vec::new();

    for h in new_holdings {
        match old_map.get(&h.name) {
            None => changes.push(HoldingChange {
                name: h.name.clone(),
                change_type: HoldingChangeType::Added,
                old_weight: None,
                new_weight: Some(h.weighting),
            }),
            Some(&old_w) if (old_w - h.weighting).abs() > 0.01 => {
                changes.push(HoldingChange {
                    name: h.name.clone(),
                    change_type: HoldingChangeType::WeightChanged,
                    old_weight: Some(old_w),
                    new_weight: Some(h.weighting),
                });
            }
            _ => {}
        }
    }

    for (name, old_w) in &old_map {
        if !new_map.contains_key(name.as_str()) {
            changes.push(HoldingChange {
                name: name.clone(),
                change_type: HoldingChangeType::Removed,
                old_weight: Some(*old_w),
                new_weight: None,
            });
        }
    }

    changes.sort_by(|a, b| {
        let type_ord = |ct: &HoldingChangeType| match ct {
            HoldingChangeType::Added => 0,
            HoldingChangeType::Removed => 1,
            HoldingChangeType::WeightChanged => 2,
        };
        type_ord(&a.change_type)
            .cmp(&type_ord(&b.change_type))
            .then_with(|| {
                let w_a = a.new_weight.or(a.old_weight).unwrap_or(0.0);
                let w_b = b.new_weight.or(b.old_weight).unwrap_or(0.0);
                w_b.partial_cmp(&w_a).unwrap_or(std::cmp::Ordering::Equal)
            })
    });

    Ok(changes)
}

fn is_equity_holding(holding: &FundHolding) -> bool {
    holding
        .ticker
        .as_deref()
        .is_some_and(|ticker| !ticker.trim().is_empty())
}

fn compute_period_metrics(
    prices: &[(String, f64)],
    benchmark_returns: &HashMap<String, f64>,
    start: NaiveDate,
    end: NaiveDate,
) -> Option<FundPeriodMetrics> {
    let start_str = format_date(start);
    let end_str = format_date(end);

    let window: Vec<&(String, f64)> = prices
        .iter()
        .filter(|(d, _)| d.as_str() >= start_str.as_str() && d.as_str() <= end_str.as_str())
        .collect();

    if window.len() < 2 {
        return None;
    }

    let start_price = window.first().unwrap().1;
    let end_price = window.last().unwrap().1;
    if start_price <= 0.0 {
        return None;
    }
    let total_return = (end_price / start_price - 1.0) * 100.0;
    let cagr = metrics::compute_cagr(
        window.first().unwrap().0.as_str(),
        window.last().unwrap().0.as_str(),
        start_price,
        end_price,
    );

    let window_prices: Vec<(String, f64)> = window.iter().map(|(d, p)| (d.clone(), *p)).collect();
    let fund_returns = metrics::compute_log_returns(&window_prices);
    let nav_values: Vec<f64> = window.iter().map(|(_, p)| *p).collect();

    let daily_returns: Vec<f64> = {
        let mut sorted_dates: Vec<&String> = fund_returns.keys().collect();
        sorted_dates.sort();
        sorted_dates.iter().map(|d| fund_returns[*d]).collect()
    };

    let volatility = metrics::compute_volatility(&daily_returns);
    let sharpe = metrics::compute_sharpe(&daily_returns);
    let sortino = metrics::compute_sortino(&daily_returns);
    let max_drawdown = metrics::compute_max_drawdown(&nav_values);

    let (aligned_fund, aligned_bench) =
        metrics::align_return_series(&fund_returns, benchmark_returns);
    let beta = metrics::compute_beta(&aligned_fund, &aligned_bench);

    Some(FundPeriodMetrics {
        total_return,
        cagr,
        volatility,
        sharpe,
        sortino,
        max_drawdown,
        beta,
    })
}

fn compute_trailing_period_metrics(
    prices: &[(String, f64)],
    benchmark_returns: &HashMap<String, f64>,
    trading_days: usize,
) -> Option<FundPeriodMetrics> {
    if prices.len() < 2 {
        return None;
    }

    let window_start = prices.len().saturating_sub(trading_days + 1);
    let start = NaiveDate::parse_from_str(&prices[window_start].0, DATE_FORMAT).ok()?;
    let end = NaiveDate::parse_from_str(&prices.last()?.0, DATE_FORMAT).ok()?;

    compute_period_metrics(prices, benchmark_returns, start, end)
}

pub fn compute_breakdown(
    holdings: &[FundHolding],
    field_fn: impl Fn(&FundHolding) -> Option<String>,
) -> Vec<AllocationEntry> {
    let mut map: HashMap<String, f64> = HashMap::new();
    for h in holdings {
        let label = field_fn(h).filter(|value| !value.trim().is_empty());
        *map.entry(label.unwrap_or_else(|| "Unclassified".to_string()))
            .or_default() += h.weighting;
    }

    let total: f64 = holdings.iter().map(|holding| holding.weighting).sum();
    if total > 0.0 {
        for v in map.values_mut() {
            *v = *v / total * 100.0;
        }
    }

    let mut entries: Vec<AllocationEntry> = map
        .into_iter()
        .map(|(label, weight)| AllocationEntry { label, weight })
        .collect();
    entries.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    entries
}

async fn compute_snapshot_diff(
    db: &DatabaseConnection,
    ms_code: &str,
    snapshot_date: Option<&str>,
    holdings: &[FundHolding],
    total_holdings: Option<i32>,
    today_str: &str,
) -> anyhow::Result<(bool, Option<String>, Vec<HoldingChange>)> {
    let fingerprint = compute_fingerprint(holdings);
    let previous = fund_holdings_snapshot_repo::find_latest(db, ms_code).await?;

    let holdings_json = serde_json::to_string(
        &holdings
            .iter()
            .map(|h| {
                serde_json::json!({
                    "name": h.name,
                    "weighting": h.weighting,
                    "ticker": h.ticker,
                    "sector": h.sector,
                    "country": h.country,
                    "currency": h.currency,
                })
            })
            .collect::<Vec<_>>(),
    )?;

    let snapshot_date = snapshot_date
        .map(str::trim)
        .filter(|date| !date.is_empty())
        .unwrap_or(today_str);

    if let Some(existing_snapshot) =
        fund_holdings_snapshot_repo::find_by_snapshot_date(db, ms_code, snapshot_date).await?
    {
        let holdings_changed = existing_snapshot.fingerprint != fingerprint;
        let holding_diff = if holdings_changed {
            compute_holding_diff(&existing_snapshot.holdings_json, holdings)?
        } else {
            Vec::new()
        };

        return Ok((
            holdings_changed,
            Some(existing_snapshot.snapshot_date),
            holding_diff,
        ));
    }

    let (holdings_changed, last_snapshot_date, holding_diff) = match &previous {
        Some(prev) => {
            let changed = prev.fingerprint != fingerprint || prev.snapshot_date != snapshot_date;
            let diff = if prev.fingerprint == fingerprint {
                Vec::new()
            } else {
                compute_holding_diff(&prev.holdings_json, holdings)?
            };
            (changed, Some(prev.snapshot_date.clone()), diff)
        }
        None => (true, None, Vec::new()),
    };

    fund_holdings_snapshot_repo::insert(
        db,
        ms_code,
        snapshot_date,
        &fingerprint,
        &holdings_json,
        total_holdings,
    )
    .await?;

    Ok((holdings_changed, last_snapshot_date, holding_diff))
}
async fn fetch_fund_data(ms_code: &str, limit: u32) -> anyhow::Result<FundData> {
    let scripts_dir = resolve_scripts_dir()?;
    let script = scripts_dir.join("get_fund_data.py");

    let output = tokio::process::Command::new("uv")
        .arg("run")
        .arg(&script)
        .arg(ms_code)
        .arg(limit.to_string())
        .output()
        .await
        .context("failed to run get_fund_data.py via uv")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("get_fund_data.py failed for {ms_code}: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).context("failed to parse get_fund_data.py output")?;

    let fund_currency = parsed["fund_currency"].as_str().map(str::to_owned);
    let total_holdings = parsed["total_holdings"].as_i64().map(|v| v as i32);
    let portfolio_date = parsed["portfolio_date"].as_str().map(str::to_owned);

    let holdings_arr = parsed["holdings"].as_array();
    let holdings: Vec<FundHolding> = holdings_arr
        .map(|arr| {
            arr.iter()
                .map(|entry| FundHolding {
                    name: entry["securityName"].as_str().unwrap_or("").to_owned(),
                    weighting: entry["weighting"].as_f64().unwrap_or(0.0),
                    ticker: entry["ticker"].as_str().map(str::to_owned),
                    sector: entry["sector"].as_str().map(str::to_owned),
                    country: entry["country"].as_str().map(str::to_owned),
                    currency: entry["currency"].as_str().map(str::to_owned),
                })
                .collect()
        })
        .unwrap_or_default();

    Ok(FundData {
        fund_currency,
        total_holdings,
        portfolio_date,
        holdings,
    })
}
