use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};

use crate::constants::{
    format_date, DATE_FORMAT, FIVE_YEAR_TRADING_DAYS, ONE_YEAR_TRADING_DAYS,
    THREE_YEAR_TRADING_DAYS,
};
use crate::models::{BenchmarkReferenceInfo, FundPeriodMetrics, MarketDataLimitation};
use crate::services::market_data::{MarketData, SourceObservation};
use crate::services::metrics;
use sea_orm::DatabaseConnection;

pub struct StandardFundMetrics {
    pub ytd: Option<FundPeriodMetrics>,
    pub one_year: Option<FundPeriodMetrics>,
    pub three_year: Option<FundPeriodMetrics>,
    pub five_year: Option<FundPeriodMetrics>,
    pub all_time: Option<FundPeriodMetrics>,
    pub benchmark_reference: BenchmarkReferenceInfo,
    pub benchmark_limitations: Vec<MarketDataLimitation>,
}

pub async fn compute_standard_fund_metrics(
    db: &DatabaseConnection,
    market_data: &MarketData,
    prices: &[(String, f64)],
    currency: Option<&str>,
    today: NaiveDate,
) -> anyhow::Result<StandardFundMetrics> {
    let requested_start = prices
        .first()
        .map_or_else(|| format_date(today), |(date, _)| date.clone());
    let benchmark = market_data
        .benchmark_reference_market_data(db, &requested_start, &format_date(today))
        .await?;
    let (base_prices, mut benchmark_limitations) = match currency {
        Some(currency) => {
            market_data
                .prepare_base_currency_prices(db, currency, prices, &format_date(today))
                .await?
        }
        None => (Vec::new(), Vec::new()),
    };
    for limitation in benchmark.limitations {
        if !benchmark_limitations.contains(&limitation) {
            benchmark_limitations.push(limitation);
        }
    }
    let benchmark_prices = benchmark.benchmark_series;
    let ytd_start = NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 always valid");

    Ok(StandardFundMetrics {
        ytd: compute_period_metrics(prices, &base_prices, &benchmark_prices, ytd_start, today),
        one_year: compute_trailing_period_metrics(
            prices,
            &base_prices,
            &benchmark_prices,
            ONE_YEAR_TRADING_DAYS,
        ),
        three_year: compute_trailing_period_metrics(
            prices,
            &base_prices,
            &benchmark_prices,
            THREE_YEAR_TRADING_DAYS,
        ),
        five_year: compute_trailing_period_metrics(
            prices,
            &base_prices,
            &benchmark_prices,
            FIVE_YEAR_TRADING_DAYS,
        ),
        all_time: compute_all_time_metrics(prices, &base_prices, &benchmark_prices, today),
        benchmark_reference: benchmark.benchmark_reference,
        benchmark_limitations,
    })
}

pub fn format_source_observations(observations: Vec<SourceObservation>) -> Vec<(String, f64)> {
    observations
        .into_iter()
        .map(|observation| (format_date(observation.date), observation.value))
        .collect()
}

fn compute_all_time_metrics(
    prices: &[(String, f64)],
    base_prices: &[(String, f64)],
    benchmark_prices: &[(String, f64)],
    today: NaiveDate,
) -> Option<FundPeriodMetrics> {
    let start = NaiveDate::parse_from_str(&prices.first()?.0, DATE_FORMAT).ok()?;
    compute_period_metrics(prices, base_prices, benchmark_prices, start, today)
}

fn compute_trailing_period_metrics(
    prices: &[(String, f64)],
    base_prices: &[(String, f64)],
    benchmark_prices: &[(String, f64)],
    trading_days: usize,
) -> Option<FundPeriodMetrics> {
    if prices.len() < 2 {
        return None;
    }

    let window_start = prices.len().saturating_sub(trading_days + 1);
    let start = NaiveDate::parse_from_str(&prices[window_start].0, DATE_FORMAT).ok()?;
    let end = NaiveDate::parse_from_str(&prices.last()?.0, DATE_FORMAT).ok()?;

    compute_period_metrics(prices, base_prices, benchmark_prices, start, end)
}

fn compute_period_metrics(
    prices: &[(String, f64)],
    base_prices: &[(String, f64)],
    benchmark_prices: &[(String, f64)],
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

    let window_prices: Vec<(String, f64)> = window.iter().map(|(d, p)| (d.clone(), *p)).collect();
    let fund_returns = metrics::compute_log_returns(&window_prices);
    let daily_returns = sorted_returns(&fund_returns);
    let nav_values: Vec<f64> = window.iter().map(|(_, p)| *p).collect();
    let beta = compute_beta_over_observation_intervals(&window, base_prices, benchmark_prices);

    Some(FundPeriodMetrics {
        total_return: (end_price / start_price - 1.0) * 100.0,
        cagr: metrics::compute_cagr(
            window.first().unwrap().0.as_str(),
            window.last().unwrap().0.as_str(),
            start_price,
            end_price,
        ),
        volatility: metrics::compute_volatility(&daily_returns),
        sharpe: metrics::compute_sharpe(&daily_returns),
        sortino: metrics::compute_sortino(&daily_returns),
        max_drawdown: metrics::compute_max_drawdown(&nav_values),
        beta,
    })
}

fn compute_beta_over_observation_intervals(
    subject_window: &[&(String, f64)],
    subject_base_prices: &[(String, f64)],
    benchmark_prices: &[(String, f64)],
) -> Option<f64> {
    if subject_window.len() < 2 {
        return None;
    }
    let subject_base: HashMap<&str, f64> = subject_base_prices
        .iter()
        .map(|(date, price)| (date.as_str(), *price))
        .collect();
    let benchmark: HashMap<&str, f64> = benchmark_prices
        .iter()
        .map(|(date, price)| (date.as_str(), *price))
        .collect();
    let observed_dates: HashSet<&str> = subject_window
        .iter()
        .map(|(date, _)| date.as_str())
        .collect();
    let mut subject_returns = Vec::with_capacity(subject_window.len() - 1);
    let mut benchmark_returns = Vec::with_capacity(subject_window.len() - 1);
    for pair in subject_window.windows(2) {
        let start_date = pair[0].0.as_str();
        let end_date = pair[1].0.as_str();
        let start = NaiveDate::parse_from_str(start_date, DATE_FORMAT).ok()?;
        let end = NaiveDate::parse_from_str(end_date, DATE_FORMAT).ok()?;
        let mut date = start + chrono::Duration::days(1);
        while date < end {
            if !matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
                && !observed_dates.contains(format_date(date).as_str())
            {
                // No subject holiday/coverage provenance is available here,
                // so an unobserved weekday is conservatively incomplete.
                return None;
            }
            date += chrono::Duration::days(1);
        }
        let subject_start = *subject_base.get(start_date)?;
        let subject_end = *subject_base.get(end_date)?;
        let benchmark_start = *benchmark.get(start_date)?;
        let benchmark_end = *benchmark.get(end_date)?;
        if [subject_start, subject_end, benchmark_start, benchmark_end]
            .iter()
            .any(|price| !price.is_finite() || *price <= 0.0)
        {
            return None;
        }
        subject_returns.push((subject_end / subject_start).ln());
        benchmark_returns.push((benchmark_end / benchmark_start).ln());
    }
    metrics::compute_beta(&subject_returns, &benchmark_returns)
}

fn sorted_returns(returns: &HashMap<String, f64>) -> Vec<f64> {
    let mut dates: Vec<&String> = returns.keys().collect();
    dates.sort();
    dates.iter().map(|date| returns[*date]).collect()
}
