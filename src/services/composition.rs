use std::collections::HashMap;

use futures::future::join_all;
use sea_orm::DatabaseConnection;

use crate::models::{
    AllocationEntry, AssetType, CompositionResult, FundHolding, MarketCapCategory, TopHolding,
};
use crate::services::market_data::MarketData;
use crate::services::portfolio::get_asset_positions;
use crate::services::price::PriceFetcher;

const LARGE_CAP_THRESHOLD: f64 = 10_000_000_000.0;
const MID_CAP_THRESHOLD: f64 = 2_000_000_000.0;

#[allow(clippy::too_many_lines)]
pub async fn compute_composition(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<CompositionResult> {
    let portfolio = get_asset_positions(db, market_data).await?;
    let total_value = portfolio.total_current_value;

    if total_value <= 0.0 {
        return Ok(CompositionResult {
            asset_class_breakdown: Vec::new(),
            equity_style_breakdown: Vec::new(),
            management_breakdown: Vec::new(),
            sector_breakdown: Vec::new(),
            country_breakdown: Vec::new(),
            market_cap_breakdown: Vec::new(),
            top_holdings: Vec::new(),
            warnings: vec!["Portfolio has no value.".to_owned()],
        });
    }

    let total_equity_value: f64 = portfolio
        .rows
        .iter()
        .filter(|p| {
            p.asset_class
                .as_deref()
                .is_some_and(|ac| ac.eq_ignore_ascii_case("equity"))
        })
        .map(|p| p.current_value)
        .sum();

    // --- Phase 1: classify each position ---
    let mut asset_class_map: HashMap<String, f64> = HashMap::new();
    let mut equity_style_map: HashMap<String, f64> = HashMap::new();
    let mut management_map: HashMap<String, f64> = HashMap::new();
    // (equity_weight, morningstar_code, display_name, ticker)
    let mut equity_fund_data: Vec<(f64, String, String, String)> = Vec::new();
    let mut direct_stock_tickers: Vec<(String, String, f64)> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    for pos in &portfolio.rows {
        let portfolio_weight = (pos.current_value / total_value) * 100.0;
        let equity_weight = if total_equity_value > 0.0 {
            (pos.current_value / total_equity_value) * 100.0
        } else {
            0.0
        };

        match pos.asset_class.as_deref() {
            Some(ac) => aggregate_into(&mut asset_class_map, ac, portfolio_weight),
            None => aggregate_into(&mut asset_class_map, "Unclassified", portfolio_weight),
        }
        if let Some(ref es) = pos.equity_style {
            aggregate_into(&mut equity_style_map, es, equity_weight);
        }
        if let Some(ref mgmt) = pos.management {
            aggregate_into(&mut management_map, mgmt, portfolio_weight);
        }

        match pos.asset_type {
            AssetType::Stock => {
                direct_stock_tickers.push((pos.ticker.clone(), pos.name.clone(), equity_weight));
            }
            AssetType::Fund | AssetType::Etf => {
                let is_equity = pos
                    .asset_class
                    .as_deref()
                    .is_some_and(|ac| ac.eq_ignore_ascii_case("equity"));
                if is_equity {
                    match pos.morningstar_code.as_deref() {
                        Some(code) => equity_fund_data.push((
                            equity_weight,
                            code.to_owned(),
                            pos.name.clone(),
                            pos.ticker.clone(),
                        )),
                        None => warnings.push(format!(
                            "No morningstar_code set for {} ({}); cannot fetch holdings",
                            pos.name, pos.ticker
                        )),
                    }
                }
            }
        }
    }

    // --- Phase 2: fetch fund holdings in parallel ---
    let fund_fetch_futures =
        equity_fund_data
            .into_iter()
            .map(|(weight, code, name, ticker)| async move {
                match market_data.fund_data(&code, 200).await {
                    Ok(fund_data) => (weight, Some(fund_data.holdings), None::<String>),
                    Err(e) => (
                        weight,
                        None,
                        Some(format!(
                            "Failed to fetch holdings for {name} ({ticker}): {e:#}"
                        )),
                    ),
                }
            });

    let mut equity_fund_holdings: Vec<(f64, Vec<FundHolding>)> = Vec::new();
    for (weight, maybe_holdings, maybe_warning) in join_all(fund_fetch_futures).await {
        if let Some(w) = maybe_warning {
            warnings.push(w);
        }
        if let Some(holdings) = maybe_holdings {
            equity_fund_holdings.push((weight, holdings));
        }
    }

    // --- Phase 3: enrich with sector, country, market cap ---
    let mut sector_map: HashMap<String, f64> = HashMap::new();
    let mut country_map: HashMap<String, f64> = HashMap::new();
    let mut market_cap_map: HashMap<String, f64> = HashMap::new();
    // Key: ticker when available, name otherwise — deduplicates the same stock across funds
    let mut holdings_map: HashMap<String, TopHolding> = HashMap::new();

    // Fund holdings: normalize within each fund so the sample (top 200) represents 100%
    for (fund_weight, holdings) in &equity_fund_holdings {
        let holdings_total: f64 = holdings.iter().map(|h| h.weighting).sum();
        if holdings_total <= 0.0 {
            continue;
        }
        for h in holdings {
            let effective_weight = fund_weight * (h.weighting / holdings_total);
            if let Some(ref s) = h.sector {
                aggregate_into(&mut sector_map, s, effective_weight);
            }
            if let Some(ref c) = h.country {
                aggregate_into(&mut country_map, c, effective_weight);
            }
            let key = h
                .ticker
                .as_deref()
                .filter(|t| !t.is_empty())
                .unwrap_or(h.name.as_str())
                .to_owned();
            let entry = holdings_map.entry(key).or_insert_with(|| TopHolding {
                name: h.name.clone(),
                ticker: h.ticker.clone().filter(|t| !t.is_empty()),
                weight: 0.0,
                country: h.country.clone(),
                sector: h.sector.clone(),
            });
            entry.weight += effective_weight;
        }
    }

    enrich_direct_stocks(
        &direct_stock_tickers,
        market_data,
        &mut sector_map,
        &mut country_map,
        &mut market_cap_map,
        &mut holdings_map,
        &mut warnings,
    )
    .await;

    let mut top_holdings: Vec<TopHolding> = holdings_map.into_values().collect();
    top_holdings.sort_by(|a, b| {
        b.weight
            .partial_cmp(&a.weight)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    top_holdings.truncate(15);

    Ok(CompositionResult {
        asset_class_breakdown: map_to_sorted_entries(asset_class_map),
        equity_style_breakdown: map_to_sorted_entries(equity_style_map),
        management_breakdown: map_to_sorted_entries(management_map),
        sector_breakdown: map_to_sorted_entries(normalize_to_100(sector_map)),
        country_breakdown: map_to_sorted_entries(normalize_to_100(country_map)),
        market_cap_breakdown: map_to_sorted_entries(normalize_to_100(market_cap_map)),
        top_holdings,
        warnings,
    })
}

// --- Private helpers ---

fn classify_market_cap(market_cap: f64) -> MarketCapCategory {
    if market_cap >= LARGE_CAP_THRESHOLD {
        MarketCapCategory::Large
    } else if market_cap >= MID_CAP_THRESHOLD {
        MarketCapCategory::Mid
    } else {
        MarketCapCategory::Small
    }
}

fn aggregate_into(map: &mut HashMap<String, f64>, label: &str, weight: f64) {
    *map.entry(label.to_owned()).or_default() += weight;
}

/// Rescales all values so they sum to 100.0.
/// Used for sector/country/market-cap maps where partial data (cash, unclassified
/// holdings) means the raw weights don't reach 100%.
fn normalize_to_100(map: HashMap<String, f64>) -> HashMap<String, f64> {
    let total: f64 = map.values().sum();
    if total <= 0.0 {
        return map;
    }
    map.into_iter()
        .map(|(k, v)| (k, v / total * 100.0))
        .collect()
}

fn map_to_sorted_entries(map: HashMap<String, f64>) -> Vec<AllocationEntry> {
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

async fn enrich_direct_stocks(
    direct_stocks: &[(String, String, f64)],
    price_fetcher: &dyn PriceFetcher,
    sector_map: &mut HashMap<String, f64>,
    country_map: &mut HashMap<String, f64>,
    market_cap_map: &mut HashMap<String, f64>,
    holdings_map: &mut HashMap<String, TopHolding>,
    warnings: &mut Vec<String>,
) {
    let fetch_futures = direct_stocks.iter().map(|(ticker, name, weight)| {
        let weight = *weight;
        let name = name.clone();
        async move {
            let info_result = price_fetcher.get_stock_info(ticker).await;
            (ticker.clone(), name, weight, info_result)
        }
    });

    for (ticker, name, weight, info_result) in join_all(fetch_futures).await {
        match info_result {
            Ok(info) => {
                if let Some(ref s) = info.sector {
                    aggregate_into(sector_map, s, weight);
                }
                if let Some(ref c) = info.country {
                    aggregate_into(country_map, c, weight);
                }
                if let Some(mc) = info.market_cap {
                    aggregate_into(market_cap_map, &classify_market_cap(mc).to_string(), weight);
                }
                let display_name = info.name.clone().unwrap_or_else(|| name.clone());
                let entry = holdings_map
                    .entry(ticker.clone())
                    .or_insert_with(|| TopHolding {
                        name: display_name,
                        ticker: Some(ticker.clone()),
                        weight: 0.0,
                        country: info.country.clone(),
                        sector: info.sector.clone(),
                    });
                entry.weight += weight;
            }
            Err(e) => {
                tracing::warn!(ticker, error = %e, "failed to fetch stock info");
                warnings.push(format!("Could not fetch info for {ticker}: {e:#}"));
                let entry = holdings_map
                    .entry(ticker.clone())
                    .or_insert_with(|| TopHolding {
                        name: name.clone(),
                        ticker: Some(ticker.clone()),
                        weight: 0.0,
                        country: None,
                        sector: None,
                    });
                entry.weight += weight;
            }
        }
    }
}
