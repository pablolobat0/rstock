use std::path::PathBuf;

use chrono::NaiveDate;

// --- App directories ---
pub fn app_data_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".rstock")
}

// --- Date ---
pub const DATE_FORMAT: &str = "%Y-%m-%d";
pub const DISPLAY_DATE_FORMAT: &str = "%d-%m-%Y";

pub fn format_date(d: NaiveDate) -> String {
    d.format(DATE_FORMAT).to_string()
}

/// Convert a YYYY-MM-DD storage string to DD-MM-YYYY for user display
pub fn display_date(storage_date: &str) -> String {
    NaiveDate::parse_from_str(storage_date, DATE_FORMAT).map_or_else(
        |_| storage_date.to_owned(),
        |d| d.format(DISPLAY_DATE_FORMAT).to_string(),
    )
}

// --- Currency ---
pub const BASE_CURRENCY: &str = "EUR";

// --- NAV ---
pub const INITIAL_NAV: f64 = 100.0;

// --- Periods (days) ---
pub const ONE_MONTH_DAYS: i64 = 30;
pub const THIRTY_DAYS: i64 = 30;
pub const THREE_MONTH_DAYS: i64 = 90;
pub const SIX_MONTH_DAYS: i64 = 182;
pub const ONE_YEAR_DAYS: i64 = 365;
pub const THREE_YEAR_DAYS: i64 = 1095;
pub const FIVE_YEAR_DAYS: i64 = 1825;
pub const ONE_YEAR_TRADING_DAYS: usize = 252;
pub const THREE_YEAR_TRADING_DAYS: usize = 756;
pub const FIVE_YEAR_TRADING_DAYS: usize = 1260;
pub const ROLLING_CORRELATION_WINDOW_DAYS: usize = 60;

// --- Metrics ---
pub const BENCHMARK_TICKER: &str = "ACWI";
pub const BENCHMARK_NAME: &str = "MSCI ACWI Benchmark";
pub const BENCHMARK_CURRENCY: &str = "USD";
pub const ANNUAL_RISK_FREE_RATE: f64 = 0.03;
pub const TRADING_DAYS_PER_YEAR: f64 = 252.0;
pub const MIN_DATA_POINTS: usize = 20;

pub fn is_benchmark_ticker(ticker: &str) -> bool {
    ticker == BENCHMARK_TICKER
}

// --- Monetary precision ---
/// Prices stored as i64 with 4 decimal places of precision.
pub const MONETARY_MULTIPLIER: f64 = 10_000.0;

// --- API ---
/// Extra days to pad fund/ETF API requests backward to avoid slow empty responses
pub const FUND_API_PADDING_DAYS: i64 = 10;

// --- Thresholds ---
pub const ZERO_RETURN_THRESHOLD: f64 = 1e-12;
pub const FLOAT_EPSILON: f64 = 1e-9;
