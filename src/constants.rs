use chrono::NaiveDate;

// --- Date ---
pub const DATE_FORMAT: &str = "%Y-%m-%d";

pub fn format_date(d: NaiveDate) -> String {
    d.format(DATE_FORMAT).to_string()
}

// --- Currency ---
pub const BASE_CURRENCY: &str = "EUR";

// --- NAV ---
pub const INITIAL_NAV: f64 = 100.0;

// --- Periods (days) ---
pub const THIRTY_DAYS: i64 = 30;
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

// --- Thresholds ---
pub const ZERO_RETURN_THRESHOLD: f64 = 1e-12;
pub const FLOAT_EPSILON: f64 = 1e-9;
