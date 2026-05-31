use std::collections::HashMap;

use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, DATE_FORMAT, MIN_DATA_POINTS};
use crate::db::repos::asset_repo;
use crate::models::{
    AlignedFundReturnPoint, AllocationComparison, AllocationEntry, CommonFundHolding,
    FundComparisonCorrelation, FundComparisonPeriod, FundComparisonResult, FundComparisonSide,
    FundHolding, FundInfoComparison, FundQuoteMetadata,
};
use crate::services::fund_analysis::{
    compute_breakdown, compute_top_n_weight, record_holdings_snapshot,
};
use crate::services::fund_metrics::{compute_standard_fund_metrics, format_source_observations};
use crate::services::market_data::MarketData;
use crate::services::metrics;

const COVERAGE_TOLERANCE_DAYS: i64 = 7;

struct FundComparisonData {
    side: FundComparisonSide,
    holdings: Vec<FundHolding>,
    prices: Vec<(String, f64)>,
}

pub async fn compare_funds(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code_a: &str,
    code_b: &str,
    period: FundComparisonPeriod,
) -> anyhow::Result<FundComparisonResult> {
    if code_a.trim().eq_ignore_ascii_case(code_b.trim()) {
        bail!("cannot compare a fund with itself; provide two different fund codes");
    }

    let (fund_a, fund_b) = tokio::try_join!(
        build_comparison_side(db, market_data, code_a),
        build_comparison_side(db, market_data, code_b),
    )?;

    let equity_a = equity_holdings(&fund_a.holdings);
    let equity_b = equity_holdings(&fund_b.holdings);

    Ok(FundComparisonResult {
        sector_allocations: compare_allocations(
            &compute_breakdown(&equity_a, |h| h.sector.clone()),
            &compute_breakdown(&equity_b, |h| h.sector.clone()),
        ),
        country_allocations: compare_allocations(
            &compute_breakdown(&equity_a, |h| h.country.clone()),
            &compute_breakdown(&equity_b, |h| h.country.clone()),
        ),
        currency_allocations: compare_allocations(
            &compute_breakdown(&equity_a, |h| h.currency.clone()),
            &compute_breakdown(&equity_b, |h| h.currency.clone()),
        ),
        common_holdings: compute_common_holdings(&fund_a.holdings, &fund_b.holdings),
        correlation: compute_correlation(&fund_a.prices, &fund_b.prices, period),
        fund_a: fund_a.side,
        fund_b: fund_b.side,
    })
}

async fn build_comparison_side(
    db: &DatabaseConnection,
    market_data: &MarketData,
    code: &str,
) -> anyhow::Result<FundComparisonData> {
    let today = chrono::Local::now().date_naive();
    let today_str = format_date(today);
    let local_name = asset_repo::find_by_morningstar_code(db, code)
        .await?
        .map(|asset| asset.name);

    let (fund_data_result, quote_metadata_result, prices_result) = tokio::join!(
        market_data.fund_data(code, 200),
        market_data.fund_quote_metadata(code),
        market_data.fund_price_history(
            code,
            NaiveDate::from_ymd_opt(2000, 1, 1).expect("literal date should be valid"),
            today,
        ),
    );

    let fund_data =
        fund_data_result.with_context(|| format!("failed to fetch holdings for {code}"))?;
    let quote_metadata = quote_metadata_or_warn(code, quote_metadata_result);
    let prices = format_source_observations(
        prices_result.with_context(|| format!("failed to fetch price history for {code}"))?,
    );
    let metrics = compute_standard_fund_metrics(market_data, &prices, today).await;
    let top_10_weight = compute_top_n_weight(&fund_data.holdings, 10);
    record_holdings_snapshot(
        db,
        code,
        fund_data.portfolio_date.as_deref(),
        &fund_data.holdings,
        fund_data.total_holdings,
        &today_str,
    )
    .await?;
    let name = local_name
        .or_else(|| {
            quote_metadata
                .as_ref()
                .and_then(|metadata| metadata.name.clone())
        })
        .unwrap_or_else(|| code.to_owned());
    let currency = quote_metadata
        .as_ref()
        .and_then(|metadata| metadata.quote_currency.clone())
        .or_else(|| fund_data.fund_currency.clone());

    let side = FundComparisonSide {
        code: code.to_owned(),
        name,
        info: FundInfoComparison {
            currency,
            aum: quote_metadata.as_ref().and_then(|metadata| metadata.aum),
            aum_currency: quote_metadata
                .as_ref()
                .and_then(|metadata| metadata.aum_currency.clone()),
            inception_date: quote_metadata
                .as_ref()
                .and_then(|metadata| metadata.inception_date.clone()),
            total_holdings: fund_data.total_holdings,
            top_10_weight,
            portfolio_date: fund_data.portfolio_date.clone(),
        },
        ytd: metrics.ytd,
        one_year: metrics.one_year,
        three_year: metrics.three_year,
        five_year: metrics.five_year,
        all_time: metrics.all_time,
    };

    Ok(FundComparisonData {
        side,
        holdings: fund_data.holdings,
        prices,
    })
}

fn quote_metadata_or_warn(
    code: &str,
    result: anyhow::Result<FundQuoteMetadata>,
) -> Option<FundQuoteMetadata> {
    result
        .map_err(|error| {
            tracing::warn!(code, error = %error, "failed to fetch fund quote metadata");
            error
        })
        .ok()
}

fn compute_correlation(
    prices_a: &[(String, f64)],
    prices_b: &[(String, f64)],
    period: FundComparisonPeriod,
) -> FundComparisonCorrelation {
    let requested_end = chrono::Local::now().date_naive();
    let requested_start = requested_end - chrono::Duration::days(period.days);
    let period_label = period.label.to_owned();

    if let Some(reason) = coverage_reason(prices_a, requested_start, requested_end, "first fund") {
        return unavailable_correlation(period_label, reason);
    }
    if let Some(reason) = coverage_reason(prices_b, requested_start, requested_end, "second fund") {
        return unavailable_correlation(period_label, reason);
    }

    let start_str = format_date(requested_start);
    let end_str = format_date(requested_end);
    let window_a = window_prices(prices_a, &start_str, &end_str);
    let window_b = window_prices(prices_b, &start_str, &end_str);
    let returns_a = metrics::compute_log_returns(&window_a);
    let returns_b = metrics::compute_log_returns(&window_b);
    let aligned_returns = metrics::align_return_series_with_dates(&returns_a, &returns_b);
    if aligned_returns.len() < MIN_DATA_POINTS {
        return unavailable_correlation(period_label, "not enough aligned return data".to_owned());
    }

    let values_a: Vec<f64> = aligned_returns
        .iter()
        .map(|(_, return_a, _)| *return_a)
        .collect();
    let values_b: Vec<f64> = aligned_returns
        .iter()
        .map(|(_, _, return_b)| *return_b)
        .collect();
    let points = aligned_return_points(&window_a, &window_b);
    if points.len() < MIN_DATA_POINTS {
        return unavailable_correlation(period_label, "not enough aligned graph data".to_owned());
    }

    FundComparisonCorrelation {
        period_label,
        correlation: Some(metrics::pearson_correlation(&values_a, &values_b)),
        reason: None,
        points,
    }
}

fn unavailable_correlation(period_label: String, reason: String) -> FundComparisonCorrelation {
    FundComparisonCorrelation {
        period_label,
        correlation: None,
        reason: Some(reason),
        points: Vec::new(),
    }
}

fn coverage_reason(
    prices: &[(String, f64)],
    requested_start: NaiveDate,
    requested_end: NaiveDate,
    label: &str,
) -> Option<String> {
    let Some((first, last)) = price_date_bounds(prices) else {
        return Some(format!("{label} has no price history"));
    };

    if first > requested_start + chrono::Duration::days(COVERAGE_TOLERANCE_DAYS) {
        return Some(format!("{label} lacks selected-period start coverage"));
    }
    if last < requested_end - chrono::Duration::days(COVERAGE_TOLERANCE_DAYS) {
        return Some(format!("{label} lacks current price coverage"));
    }
    None
}

fn price_date_bounds(prices: &[(String, f64)]) -> Option<(NaiveDate, NaiveDate)> {
    let dates: Vec<NaiveDate> = prices
        .iter()
        .filter_map(|(date, _)| NaiveDate::parse_from_str(date, DATE_FORMAT).ok())
        .collect();
    Some((*dates.iter().min()?, *dates.iter().max()?))
}

fn window_prices(prices: &[(String, f64)], start: &str, end: &str) -> Vec<(String, f64)> {
    prices
        .iter()
        .filter(|(date, _)| date.as_str() >= start && date.as_str() <= end)
        .cloned()
        .collect()
}

fn aligned_return_points(
    prices_a: &[(String, f64)],
    prices_b: &[(String, f64)],
) -> Vec<AlignedFundReturnPoint> {
    let map_b: HashMap<&str, f64> = prices_b
        .iter()
        .map(|(date, price)| (date.as_str(), *price))
        .collect();
    let mut aligned_prices: Vec<(String, f64, f64)> = prices_a
        .iter()
        .filter_map(|(date, price_a)| {
            map_b
                .get(date.as_str())
                .map(|price_b| (date.clone(), *price_a, *price_b))
        })
        .collect();
    aligned_prices.sort_by(|left, right| left.0.cmp(&right.0));

    let Some((_, base_a, base_b)) = aligned_prices.first().cloned() else {
        return Vec::new();
    };
    if base_a <= 0.0 || base_b <= 0.0 {
        return Vec::new();
    }

    aligned_prices
        .into_iter()
        .map(|(date, price_a, price_b)| AlignedFundReturnPoint {
            date,
            return_a: (price_a / base_a - 1.0) * 100.0,
            return_b: (price_b / base_b - 1.0) * 100.0,
        })
        .collect()
}

fn equity_holdings(holdings: &[FundHolding]) -> Vec<FundHolding> {
    holdings
        .iter()
        .filter(|holding| {
            holding
                .ticker
                .as_deref()
                .is_some_and(|ticker| !ticker.trim().is_empty())
        })
        .cloned()
        .collect()
}

fn compare_allocations(
    entries_a: &[AllocationEntry],
    entries_b: &[AllocationEntry],
) -> Vec<AllocationComparison> {
    let map_a: HashMap<&str, f64> = entries_a
        .iter()
        .map(|entry| (entry.label.as_str(), entry.weight))
        .collect();
    let map_b: HashMap<&str, f64> = entries_b
        .iter()
        .map(|entry| (entry.label.as_str(), entry.weight))
        .collect();
    let mut labels: Vec<String> = entries_a
        .iter()
        .map(|entry| entry.label.clone())
        .chain(entries_b.iter().map(|entry| entry.label.clone()))
        .collect();
    labels.sort();
    labels.dedup();

    let mut comparisons: Vec<AllocationComparison> = labels
        .into_iter()
        .map(|label| AllocationComparison {
            weight_a: *map_a.get(label.as_str()).unwrap_or(&0.0),
            weight_b: *map_b.get(label.as_str()).unwrap_or(&0.0),
            label,
        })
        .collect();

    comparisons.sort_by(|a, b| {
        (b.weight_a - b.weight_b)
            .abs()
            .partial_cmp(&(a.weight_a - a.weight_b).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.weight_a
                    .max(b.weight_b)
                    .partial_cmp(&a.weight_a.max(a.weight_b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.label.cmp(&b.label))
    });
    comparisons
}

pub fn compute_common_holdings(
    holdings_a: &[FundHolding],
    holdings_b: &[FundHolding],
) -> Vec<CommonFundHolding> {
    let by_ticker_b: HashMap<String, &FundHolding> = holdings_b
        .iter()
        .filter(|holding| !is_cash_holding(holding))
        .filter_map(|holding| {
            holding
                .ticker
                .as_deref()
                .map(|ticker| (ticker.trim().to_uppercase(), holding))
        })
        .filter(|(ticker, _)| !ticker.is_empty())
        .collect();
    let by_name_b: HashMap<String, &FundHolding> = holdings_b
        .iter()
        .filter(|holding| !is_cash_holding(holding))
        .map(|holding| (normalize_name(&holding.name), holding))
        .collect();

    let mut common = Vec::new();
    for holding_a in holdings_a {
        if is_cash_holding(holding_a) {
            continue;
        }

        let ticker = holding_a
            .ticker
            .as_deref()
            .map(str::trim)
            .filter(|ticker| !ticker.is_empty());
        let ticker_match =
            ticker.and_then(|ticker| by_ticker_b.get(&ticker.to_uppercase()).copied());
        let matched = match (ticker_match, ticker) {
            (Some(match_by_ticker), _) => Some(match_by_ticker),
            (None, Some(_)) => by_name_b
                .get(&normalize_name(&holding_a.name))
                .copied()
                .filter(|match_by_name| match_by_name.ticker.as_deref().is_none_or(str::is_empty)),
            (None, None) => by_name_b.get(&normalize_name(&holding_a.name)).copied(),
        };

        if let Some(holding_b) = matched {
            common.push(CommonFundHolding {
                ticker: ticker
                    .map(str::to_owned)
                    .or_else(|| holding_b.ticker.clone()),
                name_a: holding_a.name.clone(),
                weight_a: holding_a.weighting,
                weight_b: holding_b.weighting,
            });
        }
    }

    common.sort_by(|a, b| {
        b.weight_a
            .max(b.weight_b)
            .partial_cmp(&a.weight_a.max(a.weight_b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    common
}

fn is_cash_holding(holding: &FundHolding) -> bool {
    let name = normalize_name(&holding.name);
    let ticker = holding.ticker.as_deref().map(str::trim);

    name == "cash" || ticker.is_some_and(|ticker| ticker.eq_ignore_ascii_case("cash"))
}

fn normalize_name(name: &str) -> String {
    name.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
