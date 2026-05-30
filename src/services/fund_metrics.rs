use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};

use crate::constants::{
    format_date, BENCHMARK_TICKER, DATE_FORMAT, FIVE_YEAR_TRADING_DAYS, ONE_YEAR_TRADING_DAYS,
    THREE_YEAR_TRADING_DAYS,
};
use crate::models::FundPeriodMetrics;
use crate::services::market_data::{MarketData, SourceObservation};
use crate::services::metrics;

pub struct StandardFundMetrics {
    pub ytd: Option<FundPeriodMetrics>,
    pub one_year: Option<FundPeriodMetrics>,
    pub three_year: Option<FundPeriodMetrics>,
    pub five_year: Option<FundPeriodMetrics>,
    pub all_time: Option<FundPeriodMetrics>,
}

pub async fn compute_standard_fund_metrics(
    market_data: &MarketData,
    prices: &[(String, f64)],
    today: NaiveDate,
) -> StandardFundMetrics {
    let benchmark_returns = benchmark_returns(market_data, prices, today).await;
    let ytd_start = NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 always valid");

    StandardFundMetrics {
        ytd: compute_period_metrics(prices, &benchmark_returns, ytd_start, today),
        one_year: compute_trailing_period_metrics(
            prices,
            &benchmark_returns,
            ONE_YEAR_TRADING_DAYS,
        ),
        three_year: compute_trailing_period_metrics(
            prices,
            &benchmark_returns,
            THREE_YEAR_TRADING_DAYS,
        ),
        five_year: compute_trailing_period_metrics(
            prices,
            &benchmark_returns,
            FIVE_YEAR_TRADING_DAYS,
        ),
        all_time: compute_all_time_metrics(prices, &benchmark_returns, today),
    }
}

pub fn format_source_observations(observations: Vec<SourceObservation>) -> Vec<(String, f64)> {
    observations
        .into_iter()
        .map(|observation| (format_date(observation.date), observation.value))
        .collect()
}

async fn benchmark_returns(
    market_data: &MarketData,
    prices: &[(String, f64)],
    today: NaiveDate,
) -> HashMap<String, f64> {
    let Some((earliest_date, _)) = prices.first() else {
        return HashMap::new();
    };
    let Ok(benchmark_start) = NaiveDate::parse_from_str(earliest_date, DATE_FORMAT) else {
        return HashMap::new();
    };

    market_data
        .stock_price_history(BENCHMARK_TICKER, benchmark_start, today)
        .await
        .map(format_source_observations)
        .map(|prices| metrics::compute_log_returns(&prices))
        .unwrap_or_default()
}

fn compute_all_time_metrics(
    prices: &[(String, f64)],
    benchmark_returns: &HashMap<String, f64>,
    today: NaiveDate,
) -> Option<FundPeriodMetrics> {
    let start = NaiveDate::parse_from_str(&prices.first()?.0, DATE_FORMAT).ok()?;
    compute_period_metrics(prices, benchmark_returns, start, today)
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

    let window_prices: Vec<(String, f64)> = window.iter().map(|(d, p)| (d.clone(), *p)).collect();
    let fund_returns = metrics::compute_log_returns(&window_prices);
    let daily_returns = sorted_returns(&fund_returns);
    let nav_values: Vec<f64> = window.iter().map(|(_, p)| *p).collect();
    let (aligned_fund, aligned_benchmark) =
        metrics::align_return_series(&fund_returns, benchmark_returns);

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
        beta: metrics::compute_beta(&aligned_fund, &aligned_benchmark),
    })
}

fn sorted_returns(returns: &HashMap<String, f64>) -> Vec<f64> {
    let mut dates: Vec<&String> = returns.keys().collect();
    dates.sort();
    dates.iter().map(|date| returns[*date]).collect()
}
