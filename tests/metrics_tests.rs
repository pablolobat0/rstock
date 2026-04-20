use rstock::constants::{ANNUAL_RISK_FREE_RATE, MIN_DATA_POINTS, TRADING_DAYS_PER_YEAR};
use rstock::services::metrics::compute_sortino;

fn daily_risk_free_rate() -> f64 {
    (1.0 + ANNUAL_RISK_FREE_RATE).powf(1.0 / TRADING_DAYS_PER_YEAR) - 1.0
}

#[test]
fn test_sortino_none_for_insufficient_data() {
    let returns = vec![0.001; MIN_DATA_POINTS - 1];
    assert!(compute_sortino(&returns).is_none());
}

#[test]
fn test_sortino_zero_when_all_excess_returns_are_non_negative() {
    let returns = vec![0.01; MIN_DATA_POINTS];
    assert_eq!(compute_sortino(&returns), Some(0.0));
}

#[test]
fn test_sortino_matches_expected_annualized_value() {
    let rf = daily_risk_free_rate();
    let downside = -0.01_f64;
    let upside = 0.02_f64;

    let mut returns = vec![rf + downside; MIN_DATA_POINTS / 2];
    returns.extend(vec![rf + upside; MIN_DATA_POINTS / 2]);

    let sortino = compute_sortino(&returns).unwrap();
    let mean_excess = (10.0 * downside + 10.0 * upside) / MIN_DATA_POINTS as f64;
    let downside_deviation = ((10.0 * downside.powi(2)) / MIN_DATA_POINTS as f64).sqrt();
    let expected = (mean_excess / downside_deviation) * TRADING_DAYS_PER_YEAR.sqrt();

    assert!((sortino - expected).abs() < 1e-9);
}

#[test]
fn test_sortino_zero_when_downside_deviation_is_zero() {
    let rf = daily_risk_free_rate();
    let returns = vec![rf; MIN_DATA_POINTS];
    assert_eq!(compute_sortino(&returns), Some(0.0));
}

#[test]
fn test_sortino_negative_when_average_excess_return_is_negative() {
    let rf = daily_risk_free_rate();
    let mut returns = vec![rf - 0.02; MIN_DATA_POINTS - 2];
    returns.extend([rf + 0.001, rf + 0.001]);

    let sortino = compute_sortino(&returns).unwrap();
    assert!(sortino < 0.0);
}
