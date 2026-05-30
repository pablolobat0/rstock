use std::collections::HashMap;

use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::format_date;
use crate::db::repos::{asset_repo, fund_holdings_snapshot_repo};
use crate::models::{
    AllocationEntry, FundAnalysisResult, FundData, FundHolding, FundQuoteMetadata, HoldingChange,
    HoldingChangeType,
};
use crate::services::fund_metrics::{compute_standard_fund_metrics, format_source_observations};
use crate::services::market_data::MarketData;

pub async fn compute_fund_analysis(
    db: &DatabaseConnection,
    market_data: &MarketData,
    ms_code: &str,
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

    let metrics = compute_standard_fund_metrics(market_data, &fund_prices, today).await;

    let sector_breakdown = compute_breakdown(&equity_holdings, |h| h.sector.clone());
    let country_breakdown = compute_breakdown(&equity_holdings, |h| h.country.clone());
    let currency_breakdown = compute_breakdown(&equity_holdings, |h| h.currency.clone());
    let top_10_weight = compute_top_n_weight(&all_holdings, 10);

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
        ytd: metrics.ytd,
        one_year: metrics.one_year,
        three_year: metrics.three_year,
        five_year: metrics.five_year,
        all_time: metrics.all_time,
        holdings_changed,
        last_snapshot_date,
        holding_diff,
    })
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
