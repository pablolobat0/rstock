use chrono::NaiveDate;

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

// --- Metrics ---
pub const BENCHMARK_TICKER: &str = "ACWI";
pub const BENCHMARK_NAME: &str = "MSCI ACWI Benchmark";
pub const BENCHMARK_CURRENCY: &str = "USD";
pub const ANNUAL_RISK_FREE_RATE: f64 = 0.03;
pub const TRADING_DAYS_PER_YEAR: f64 = 252.0;
pub const MIN_DATA_POINTS: usize = 20;

// --- Monitor / Momentum indicators ---
pub const RSI_PERIOD: usize = 14;
pub const SMA_SHORT: usize = 50;
pub const SMA_LONG: usize = 200;
pub const MACD_FAST: usize = 12;
pub const MACD_SLOW: usize = 26;
pub const MACD_SIGNAL_PERIOD: usize = 9;
pub const RSI_OVERBOUGHT: f64 = 70.0;
pub const RSI_OVERSOLD: f64 = 30.0;
/// Extra trading days of history to fetch for SMA200 warmup
pub const MONITOR_WARMUP_DAYS: i64 = 300;

// --- Monetary precision ---
/// Prices stored as i64 with 4 decimal places of precision.
pub const MONETARY_MULTIPLIER: f64 = 10_000.0;

// --- Thresholds ---
pub const ZERO_RETURN_THRESHOLD: f64 = 1e-12;
pub const FLOAT_EPSILON: f64 = 1e-9;
