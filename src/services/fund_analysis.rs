use std::collections::HashMap;

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, BENCHMARK_TICKER, DATE_FORMAT, FIVE_YEAR_TRADING_DAYS, ONE_YEAR_TRADING_DAYS,
    THREE_YEAR_TRADING_DAYS,
};
use crate::db::repos::{
    asset_repo, fund_holdings_snapshot_repo, portfolio_asset_history_repo, portfolio_history_repo,
};
use crate::models::{
    AllocationEntry, Asset, CandidateCorrelationPeriod, CandidateCorrelationResult,
    CandidateCorrelationRow, FundAnalysisResult, FundData, FundHolding, FundPeriodMetrics,
    FundQuoteMetadata, HoldingChange, HoldingChangeType,
};
use crate::services::market_data::{MarketData, SourceObservation};
use crate::services::{analytics, metrics, portfolio};

const COVERAGE_TOLERANCE_DAYS: i64 = 7;

pub async fn compute_fund_analysis(
    db: &DatabaseConnection,
    market_data: &MarketData,
    ms_code: &str,
    correlation_period: CandidateCorrelationPeriod,
) -> anyhow::Result<FundAnalysisResult> {
    let today = chrono::Local::now().date_naive();
    let today_str = format_date(today);

    let asset = asset_repo::find_by_morningstar_code(db, ms_code).await?;
    let local_name = asset.as_ref().map(|a| a.name.clone());

    let (fund_data_result, quote_metadata_result, prices_result) = tokio::join!(
        market_data.fund_data(ms_code, 200),
        market_data.fund_quote_metadata(ms_code),
        market_data.fund_price_history(
            ms_code,
            NaiveDate::from_ymd_opt(2000, 1, 1).expect("literal date should be valid"),
            today,
        ),
    );

    let fund_data = fund_data_result?;
    let quote_metadata = quote_metadata_or_warn(ms_code, quote_metadata_result);
    let fund_prices = format_source_observations(prices_result?);

    let fund_currency = fund_currency(&fund_data, quote_metadata.as_ref());
    let name = fund_name(local_name, quote_metadata.as_ref());
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
    let benchmark_start =
        NaiveDate::parse_from_str(earliest_date, DATE_FORMAT).context("invalid fund price date")?;
    let benchmark_prices = market_data
        .stock_price_history(BENCHMARK_TICKER, benchmark_start, today)
        .await
        .map(format_source_observations)
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
    let top_10_weight = compute_top_n_weight(&all_holdings, 10);

    let candidate_correlation =
        compute_candidate_correlation(db, market_data, &fund_prices, correlation_period).await;

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
        aum: quote_metadata.as_ref().and_then(|metadata| metadata.aum),
        aum_currency: quote_metadata
            .as_ref()
            .and_then(|metadata| metadata.aum_currency.clone()),
        inception_date: quote_metadata
            .as_ref()
            .and_then(|metadata| metadata.inception_date.clone()),
        total_holdings,
        portfolio_date,
        top_10_weight,
        top_holdings,
        sector_breakdown,
        country_breakdown,
        currency_breakdown,
        ytd,
        one_year,
        three_year,
        five_year,
        all_time,
        candidate_correlation,
        holdings_changed,
        last_snapshot_date,
        holding_diff,
    })
}

async fn compute_candidate_correlation(
    db: &DatabaseConnection,
    market_data: &MarketData,
    fund_prices: &[(String, f64)],
    period: CandidateCorrelationPeriod,
) -> CandidateCorrelationResult {
    let period_label = period.label.to_owned();
    let rebuild_error = portfolio::trigger_rebuild_if_needed(db, market_data)
        .await
        .err()
        .map(|error| {
            tracing::warn!(error = %error, "failed to rebuild portfolio before fund candidate correlation");
            "portfolio rebuild failed".to_owned()
        });

    let latest = match portfolio_history_repo::find_latest(db).await {
        Ok(Some(snapshot)) => snapshot,
        Ok(None) => {
            return CandidateCorrelationResult {
                period_label,
                rows: vec![unavailable_portfolio_row("portfolio history unavailable")],
            };
        }
        Err(error) => {
            tracing::warn!(error = %error, "failed to load portfolio history for fund candidate correlation");
            return CandidateCorrelationResult {
                period_label,
                rows: vec![unavailable_portfolio_row("portfolio history unavailable")],
            };
        }
    };

    let end = match NaiveDate::parse_from_str(&latest.date, DATE_FORMAT) {
        Ok(date) => date,
        Err(error) => {
            tracing::warn!(date = latest.date, error = %error, "invalid latest portfolio NAV date");
            return CandidateCorrelationResult {
                period_label,
                rows: vec![unavailable_portfolio_row(
                    "portfolio history date unavailable",
                )],
            };
        }
    };
    let start = end - Duration::days(period.days);
    let start_str = format_date(start);
    let end_str = format_date(end);

    let candidate_window = window_prices(fund_prices, &start_str, &end_str);
    let candidate_reason = coverage_reason(&candidate_window, start, end, "candidate");
    let candidate_returns = metrics::compute_log_returns(&candidate_window);

    let mut rows = Vec::new();
    let portfolio_reason = rebuild_error.as_deref().or(candidate_reason.as_deref());
    rows.push(if let Some(reason) = portfolio_reason {
        unavailable_portfolio_row(reason)
    } else {
        compute_portfolio_correlation_row(db, &candidate_returns, start, end).await
    });

    rows.extend(
        compute_asset_correlation_rows(
            db,
            market_data,
            &candidate_returns,
            candidate_reason.as_deref(),
            &start_str,
            &end_str,
            start,
            end,
        )
        .await,
    );

    sort_candidate_correlation_rows(&mut rows);
    CandidateCorrelationResult { period_label, rows }
}

async fn compute_portfolio_correlation_row(
    db: &DatabaseConnection,
    candidate_returns: &HashMap<String, f64>,
    start: NaiveDate,
    end: NaiveDate,
) -> CandidateCorrelationRow {
    let start_str = format_date(start);
    let end_str = format_date(end);
    let snapshots = match portfolio_history_repo::find_between(db, &start_str, &end_str).await {
        Ok(snapshots) => snapshots,
        Err(error) => {
            tracing::warn!(error = %error, "failed to load portfolio NAV series for fund candidate correlation");
            return unavailable_portfolio_row("portfolio NAV unavailable");
        }
    };

    let nav_prices: Vec<(String, f64)> = snapshots
        .into_iter()
        .map(|snapshot| (snapshot.date, snapshot.nav))
        .collect();

    if let Some(reason) = coverage_reason(&nav_prices, start, end, "portfolio NAV") {
        return unavailable_portfolio_row(&reason);
    }

    let portfolio_returns = metrics::compute_log_returns(&nav_prices);
    correlation_row("Portfolio NAV", true, candidate_returns, &portfolio_returns)
}

#[allow(clippy::too_many_arguments)]
async fn compute_asset_correlation_rows(
    db: &DatabaseConnection,
    market_data: &MarketData,
    candidate_returns: &HashMap<String, f64>,
    candidate_reason: Option<&str>,
    start_str: &str,
    end_str: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> Vec<CandidateCorrelationRow> {
    let latest_assets = match portfolio_asset_history_repo::find_by_date(db, end_str).await {
        Ok(snapshots) => snapshots,
        Err(error) => {
            tracing::warn!(error = %error, "failed to load current holdings for fund candidate correlation");
            return Vec::new();
        }
    };
    let asset_ids: Vec<i32> = latest_assets
        .iter()
        .map(|snapshot| snapshot.asset_id)
        .collect();
    let assets = match asset_repo::find_by_ids(db, asset_ids.into_iter()).await {
        Ok(assets) => assets,
        Err(error) => {
            tracing::warn!(error = %error, "failed to load held assets for fund candidate correlation");
            return Vec::new();
        }
    };

    if let Some(reason) = candidate_reason {
        return assets
            .into_iter()
            .map(|asset| unavailable_asset_row(asset.name, reason))
            .collect();
    }

    let mut rows = Vec::with_capacity(assets.len());
    for asset in assets {
        rows.push(
            compute_one_asset_correlation_row(
                db,
                market_data,
                asset,
                candidate_returns,
                start_str,
                end_str,
                start,
                end,
            )
            .await,
        );
    }
    rows
}

#[allow(clippy::too_many_arguments)]
async fn compute_one_asset_correlation_row(
    db: &DatabaseConnection,
    market_data: &MarketData,
    asset: Asset,
    candidate_returns: &HashMap<String, f64>,
    start_str: &str,
    end_str: &str,
    start: NaiveDate,
    end: NaiveDate,
) -> CandidateCorrelationRow {
    let asset_name = asset.name.clone();
    let series = match market_data
        .tracked_correlation_market_data(db, vec![asset], start_str, end_str)
        .await
    {
        Ok((mut series, _limitations)) => series.pop(),
        Err(error) => {
            tracing::warn!(asset = asset_name, error = %error, "failed to load held-asset series for fund candidate correlation");
            return unavailable_asset_row(asset_name, "asset price history unavailable");
        }
    };

    let Some(series) = series else {
        return unavailable_asset_row(asset_name, "asset price history unavailable");
    };

    if let Some(reason) = coverage_reason(&series.prices, start, end, &asset_name) {
        return unavailable_asset_row(asset_name, &reason);
    }

    let asset_returns = metrics::compute_log_returns(&series.prices);
    correlation_row(&asset_name, false, candidate_returns, &asset_returns)
}

fn correlation_row(
    label: &str,
    is_portfolio: bool,
    candidate_returns: &HashMap<String, f64>,
    comparison_returns: &HashMap<String, f64>,
) -> CandidateCorrelationRow {
    match analytics::compute_return_correlation(candidate_returns, comparison_returns) {
        Some(correlation) => CandidateCorrelationRow {
            label: label.to_owned(),
            correlation: Some(correlation),
            reason: None,
            is_portfolio,
        },
        None => CandidateCorrelationRow {
            label: label.to_owned(),
            correlation: None,
            reason: Some("insufficient aligned returns".to_owned()),
            is_portfolio,
        },
    }
}

fn coverage_reason(
    prices: &[(String, f64)],
    requested_start: NaiveDate,
    requested_end: NaiveDate,
    label: &str,
) -> Option<String> {
    let first = prices
        .first()
        .and_then(|(date, _)| NaiveDate::parse_from_str(date, DATE_FORMAT).ok());
    let last = prices
        .last()
        .and_then(|(date, _)| NaiveDate::parse_from_str(date, DATE_FORMAT).ok());

    let (Some(first), Some(last)) = (first, last) else {
        return Some(format!("{label} history unavailable"));
    };

    if first > requested_start + Duration::days(COVERAGE_TOLERANCE_DAYS) {
        return Some(format!("{label} lacks start coverage"));
    }
    if last < requested_end - Duration::days(COVERAGE_TOLERANCE_DAYS) {
        return Some(format!("{label} lacks recent coverage"));
    }
    None
}

fn window_prices(prices: &[(String, f64)], start_str: &str, end_str: &str) -> Vec<(String, f64)> {
    prices
        .iter()
        .filter(|(date, _)| date.as_str() >= start_str && date.as_str() <= end_str)
        .cloned()
        .collect()
}

fn unavailable_portfolio_row(reason: &str) -> CandidateCorrelationRow {
    CandidateCorrelationRow {
        label: "Portfolio NAV".to_owned(),
        correlation: None,
        reason: Some(reason.to_owned()),
        is_portfolio: true,
    }
}

fn unavailable_asset_row(label: String, reason: &str) -> CandidateCorrelationRow {
    CandidateCorrelationRow {
        label,
        correlation: None,
        reason: Some(reason.to_owned()),
        is_portfolio: false,
    }
}

fn sort_candidate_correlation_rows(rows: &mut [CandidateCorrelationRow]) {
    rows.sort_by(|left, right| {
        right.is_portfolio.cmp(&left.is_portfolio).then_with(|| {
            match (left.correlation, right.correlation) {
                (Some(a), Some(b)) => b.partial_cmp(&a).unwrap_or(std::cmp::Ordering::Equal),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => left.label.cmp(&right.label),
            }
        })
    });
}

fn quote_metadata_or_warn(
    ms_code: &str,
    result: anyhow::Result<FundQuoteMetadata>,
) -> Option<FundQuoteMetadata> {
    result
        .map_err(|error| {
            tracing::warn!(ms_code, error = %error, "failed to fetch fund quote metadata");
            error
        })
        .ok()
}

fn fund_currency(
    fund_data: &FundData,
    quote_metadata: Option<&FundQuoteMetadata>,
) -> Option<String> {
    quote_metadata
        .and_then(|metadata| metadata.quote_currency.clone())
        .or_else(|| fund_data.fund_currency.clone())
}

fn fund_name(
    local_name: Option<String>,
    quote_metadata: Option<&FundQuoteMetadata>,
) -> Option<String> {
    local_name.or_else(|| quote_metadata.and_then(|metadata| metadata.name.clone()))
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

fn format_source_observations(observations: Vec<SourceObservation>) -> Vec<(String, f64)> {
    observations
        .into_iter()
        .map(|observation| (format_date(observation.date), observation.value))
        .collect()
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

pub fn compute_top_n_weight(holdings: &[FundHolding], count: usize) -> Option<f64> {
    if holdings.is_empty() || count == 0 {
        return None;
    }

    let mut weights: Vec<f64> = holdings.iter().map(|holding| holding.weighting).collect();
    weights.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    Some(weights.into_iter().take(count).sum())
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
