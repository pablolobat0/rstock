use chrono::{Datelike, Duration, NaiveDate};
use rstock::models::{BenchmarkComparisonPeriod, ComparisonFact, FundBenchmarkPeriod};
use rstock::services::benchmark::{
    compare_aligned_eur_series, compute_standard_fund_benchmark_periods,
};

type PriceSeries = Vec<(String, f64)>;

#[test]
fn aligned_comparison_rebases_both_series_and_reports_cumulative_returns() {
    let fund = vec![
        ("2025-01-06".to_owned(), 100.0),
        ("2025-01-07".to_owned(), 112.0),
    ];
    let reference = vec![
        ("2025-01-06".to_owned(), 50.0),
        ("2025-01-07".to_owned(), 54.5),
    ];
    let (fund_wealth, reference_wealth, fund_return, reference_return) =
        compare_aligned_eur_series(&fund, &reference, "2025-01-06", "2025-01-07")
            .unwrap()
            .unwrap();

    assert_eq!(fund_wealth[0].wealth, 100.0);
    assert_eq!(reference_wealth[0].wealth, 100.0);
    assert!((fund_return - 12.0).abs() < 1e-10);
    assert!((reference_return - 9.0).abs() < 1e-10);
    assert!((fund_return - reference_return - 3.0).abs() < 1e-10);
    assert_eq!(
        fund_wealth.last().unwrap().date,
        reference_wealth.last().unwrap().date
    );
}

#[test]
fn aligned_comparison_rejects_an_internal_weekday_gap() {
    let fund = vec![
        ("2025-01-06".to_owned(), 100.0),
        ("2025-01-08".to_owned(), 102.0),
    ];
    let reference = vec![
        ("2025-01-06".to_owned(), 50.0),
        ("2025-01-07".to_owned(), 51.0),
        ("2025-01-08".to_owned(), 52.0),
    ];
    assert!(
        compare_aligned_eur_series(&fund, &reference, "2025-01-06", "2025-01-08")
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("internal weekday observation missing")
    );
}

#[test]
fn aligned_comparison_does_not_shorten_missing_anchor_or_end_with_two_common_dates() {
    let fund = vec![
        ("2025-01-06".to_owned(), 100.0),
        ("2025-01-07".to_owned(), 101.0),
        ("2025-01-08".to_owned(), 102.0),
        ("2025-01-09".to_owned(), 103.0),
    ];
    let late_reference_start = vec![
        ("2025-01-08".to_owned(), 50.0),
        ("2025-01-09".to_owned(), 51.0),
    ];
    let late = compare_aligned_eur_series(&fund, &late_reference_start, "2025-01-06", "2025-01-09")
        .unwrap();
    assert!(late.unwrap_err().to_string().contains("anchor 2025-01-06"));

    let missing_reference_end = vec![
        ("2025-01-06".to_owned(), 50.0),
        ("2025-01-07".to_owned(), 50.5),
        ("2025-01-08".to_owned(), 51.0),
    ];
    let missing_end =
        compare_aligned_eur_series(&fund, &missing_reference_end, "2025-01-06", "2025-01-09")
            .unwrap();
    assert!(missing_end
        .unwrap_err()
        .to_string()
        .contains("end 2025-01-09"));
}

#[test]
fn standard_comparison_metrics_use_hand_checkable_aligned_daily_returns() {
    let dates = weekdays("2025-10-01", 21);
    let mut subject = vec![(dates[0].clone(), 100.0)];
    let mut reference = vec![(dates[0].clone(), 100.0)];
    let mut subject_returns = Vec::new();
    let mut reference_returns = Vec::new();
    for index in 0..20 {
        let reference_return = if index % 2 == 0 { 0.01 } else { 0.02 };
        let subject_return = if index % 2 == 0 { 0.03 } else { 0.01 };
        reference_returns.push(reference_return);
        subject_returns.push(subject_return);
        subject.push((
            dates[index + 1].clone(),
            subject.last().unwrap().1 * (1.0 + subject_return),
        ));
        reference.push((
            dates[index + 1].clone(),
            reference.last().unwrap().1 * (1.0 + reference_return),
        ));
    }

    let periods = compute_standard_fund_benchmark_periods(
        &subject,
        &reference,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let since = periods
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::SinceComparisonStart)
        .unwrap();

    let daily_active: Vec<f64> = subject_returns
        .iter()
        .zip(&reference_returns)
        .map(|(subject, reference)| subject - reference)
        .collect();
    let active_mean = daily_active.iter().sum::<f64>() / daily_active.len() as f64;
    let active_std = (daily_active
        .iter()
        .map(|value| (value - active_mean).powi(2))
        .sum::<f64>()
        / (daily_active.len() - 1) as f64)
        .sqrt();
    let expected_tracking_error = active_std * 252.0_f64.sqrt() * 100.0;
    let expected_information_ratio = active_mean / active_std * 252.0_f64.sqrt();
    let log_subject: Vec<f64> = subject_returns.iter().map(|value| value.ln_1p()).collect();
    let log_reference: Vec<f64> = reference_returns
        .iter()
        .map(|value| value.ln_1p())
        .collect();
    let expected_beta =
        sample_covariance(&log_subject, &log_reference) / sample_variance(&log_reference);
    let expected_correlation = sample_covariance(&log_subject, &log_reference)
        / (sample_variance(&log_subject) * sample_variance(&log_reference)).sqrt();

    assert_eq!(since.start_date.as_deref(), Some(dates[0].as_str()));
    assert_eq!(since.end_date.as_deref(), Some(dates[20].as_str()));
    assert_close(
        available(&since.fund_return_pct),
        (subject[20].1 / 100.0 - 1.0) * 100.0,
    );
    assert_close(
        available(&since.reference_return_pct),
        (reference[20].1 / 100.0 - 1.0) * 100.0,
    );
    assert_close(
        available(&since.excess_return_percentage_points),
        (subject[20].1 / 100.0 - reference[20].1 / 100.0) * 100.0,
    );
    assert_close(available(&since.beta), expected_beta);
    assert_close(available(&since.correlation), expected_correlation);
    assert_close(
        available(&since.tracking_error_annualized_pct),
        expected_tracking_error,
    );
    assert_close(
        available(&since.information_ratio),
        expected_information_ratio,
    );

    // Hand-checkable values for the same twenty alternating intervals.
    assert!(
        (available(&since.tracking_error_annualized_pct) - 24.430_352).abs() < 1e-6,
        "{}",
        available(&since.tracking_error_annualized_pct)
    );
    assert!(
        (available(&since.information_ratio) - 5.157_519).abs() < 1e-6,
        "{}",
        available(&since.information_ratio)
    );
    assert!(
        (available(&since.beta) + 1.990_244).abs() < 1e-6,
        "{}",
        available(&since.beta)
    );
    assert!(
        (available(&since.correlation) + 1.0).abs() < 1e-9,
        "{}",
        available(&since.correlation)
    );
}

#[test]
fn standard_comparison_distinguishes_short_history_zero_variance_and_internal_gaps() {
    let dates = weekdays("2025-01-06", 22);
    let subject = dates
        .iter()
        .enumerate()
        .map(|(index, date)| (date.clone(), 100.0 * 1.011_f64.powi(index as i32)))
        .collect::<Vec<_>>();
    let reference = dates
        .iter()
        .enumerate()
        .map(|(index, date)| (date.clone(), 100.0 * 1.01_f64.powi(index as i32)))
        .collect::<Vec<_>>();
    let periods = compute_standard_fund_benchmark_periods(
        &subject,
        &reference,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let since = periods
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::SinceComparisonStart)
        .unwrap();
    assert_eq!(available(&since.tracking_error_annualized_pct), 0.0);
    assert!(matches!(
        since.information_ratio,
        ComparisonFact::NotApplicable { .. }
    ));
    assert!(matches!(since.beta, ComparisonFact::NotApplicable { .. }));
    assert!(matches!(
        since.correlation,
        ComparisonFact::NotApplicable { .. }
    ));
    let varied_subject = dates
        .iter()
        .enumerate()
        .map(|(index, date)| (date.clone(), if index % 2 == 0 { 100.0 } else { 102.0 }))
        .collect::<Vec<_>>();
    let zero_variance_benchmark = compute_standard_fund_benchmark_periods(
        &varied_subject,
        &reference,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let zero_benchmark_since = zero_variance_benchmark
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::SinceComparisonStart)
        .unwrap();
    assert!(matches!(
        zero_benchmark_since.beta,
        ComparisonFact::NotApplicable { .. }
    ));
    assert!(matches!(
        zero_benchmark_since.correlation,
        ComparisonFact::NotApplicable { .. }
    ));
    let zero_variance_subject = compute_standard_fund_benchmark_periods(
        &reference,
        &varied_subject,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let zero_subject_since = zero_variance_subject
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::SinceComparisonStart)
        .unwrap();
    assert!(matches!(
        zero_subject_since.beta,
        ComparisonFact::Available { value } if value.abs() < 1e-9
    ));
    assert!(matches!(
        zero_subject_since.correlation,
        ComparisonFact::NotApplicable { .. }
    ));
    let one_year = periods
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::OneYear)
        .unwrap();
    assert!(matches!(
        one_year.fund_return_pct,
        ComparisonFact::NotApplicable { .. }
    ));

    let mut gapped_subject = subject.clone();
    gapped_subject.remove(10);
    let gapped = compute_standard_fund_benchmark_periods(
        &gapped_subject,
        &reference,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let gapped_since = gapped
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::SinceComparisonStart)
        .unwrap();
    assert!(matches!(
        gapped_since.fund_return_pct,
        ComparisonFact::Unavailable { .. }
    ));
    assert!(matches!(
        gapped_since.tracking_error_annualized_pct,
        ComparisonFact::Unavailable { .. }
    ));

    let missing_reference =
        compute_standard_fund_benchmark_periods(&subject, &[], &dates[0], dates.last().unwrap())
            .unwrap();
    let missing_since = missing_reference
        .iter()
        .find(|period| period.period == BenchmarkComparisonPeriod::SinceComparisonStart)
        .unwrap();
    assert!(matches!(
        missing_since.reference_return_pct,
        ComparisonFact::Unavailable { .. }
    ));
}

#[test]
fn standard_comparison_rejects_nonfinite_inputs() {
    let subject = vec![
        ("2025-01-06".to_owned(), 100.0),
        ("2025-01-07".to_owned(), f64::INFINITY),
    ];
    let reference = vec![
        ("2025-01-06".to_owned(), 100.0),
        ("2025-01-07".to_owned(), 101.0),
    ];
    assert!(compute_standard_fund_benchmark_periods(
        &subject,
        &reference,
        "2025-01-06",
        "2025-01-07",
    )
    .is_err());
}

#[test]
fn ytd_period_requires_the_subject_anchor_instead_of_shortening_the_window() {
    let dates = weekdays("2025-01-06", 5);
    let subject = growing_series(&dates, 100.0, 0.01);
    let reference = growing_series(&dates, 50.0, 0.005);
    // The reference misses the subject anchor close and is complete from the
    // next day: the year-to-date window cannot start later without dropping a
    // required subject observation.
    let late_reference = reference[1..].to_vec();

    let periods = compute_standard_fund_benchmark_periods(
        &subject,
        &late_reference,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let ytd = standard_period(&periods, BenchmarkComparisonPeriod::Ytd);
    assert!(matches!(
        ytd.fund_return_pct,
        ComparisonFact::Unavailable { .. }
    ));
    assert!(matches!(
        ytd.tracking_error_annualized_pct,
        ComparisonFact::Unavailable { .. }
    ));
    assert_eq!(ytd.start_date, None);
    assert_eq!(ytd.requested_start_date.as_deref(), Some("2025-01-01"));

    let covered = compute_standard_fund_benchmark_periods(
        &subject,
        &reference,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let covered_ytd = standard_period(&covered, BenchmarkComparisonPeriod::Ytd);
    assert_eq!(covered_ytd.start_date.as_deref(), Some("2025-01-06"));
    assert!(matches!(
        covered_ytd.fund_return_pct,
        ComparisonFact::Available { .. }
    ));
}

#[test]
fn ytd_window_excludes_observations_before_the_approved_comparison_start() {
    let dates = weekdays("2024-12-27", 11);
    let subject = growing_series(&dates, 100.0, 0.01);
    let reference = growing_series(&dates, 50.0, 0.005);

    let periods =
        compute_standard_fund_benchmark_periods(&subject, &reference, "2025-01-06", "2025-01-10")
            .unwrap();
    let ytd = standard_period(&periods, BenchmarkComparisonPeriod::Ytd);
    assert_eq!(ytd.start_date.as_deref(), Some("2025-01-06"));
    assert_eq!(ytd.end_date.as_deref(), Some("2025-01-10"));
    // The bounded window drives the return; the earlier observations are not
    // admitted into the year-to-date period.
    let anchor_index = subject
        .iter()
        .position(|(date, _)| date == "2025-01-06")
        .unwrap();
    assert_close(
        available(&ytd.fund_return_pct),
        (subject.last().unwrap().1 / subject[anchor_index].1 - 1.0) * 100.0,
    );
}

#[test]
fn ytd_period_keeps_the_valid_subject_inception_fallback() {
    let dates = weekdays("2025-03-03", 5);
    let subject = growing_series(&dates, 100.0, 0.01);
    let reference = growing_series(&dates, 50.0, 0.005);

    let periods = compute_standard_fund_benchmark_periods(
        &subject,
        &reference,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let ytd = standard_period(&periods, BenchmarkComparisonPeriod::Ytd);
    assert_eq!(ytd.start_date.as_deref(), Some("2025-03-03"));
    assert!(matches!(
        ytd.fund_return_pct,
        ComparisonFact::Available { .. }
    ));
}

#[test]
fn standard_comparison_reports_complete_trailing_periods_and_weekend_anchors() {
    let dates = weekdays_between("2020-12-01", "2025-12-31");
    let (subject, reference) = varying_growth(&dates);

    let periods = compute_standard_fund_benchmark_periods(
        &subject,
        &reference,
        &dates[0],
        dates.last().unwrap(),
    )
    .unwrap();
    let ytd = standard_period(&periods, BenchmarkComparisonPeriod::Ytd);
    assert_eq!(ytd.start_date.as_deref(), Some("2025-01-01"));
    let one_year = standard_period(&periods, BenchmarkComparisonPeriod::OneYear);
    assert_eq!(one_year.start_date.as_deref(), Some("2024-12-31"));
    let three_year = standard_period(&periods, BenchmarkComparisonPeriod::ThreeYear);
    // 2022-12-31 is a Saturday, so the trailing anchor is the preceding close.
    assert_eq!(three_year.start_date.as_deref(), Some("2022-12-30"));
    let five_year = standard_period(&periods, BenchmarkComparisonPeriod::FiveYear);
    assert_eq!(five_year.start_date.as_deref(), Some("2020-12-31"));

    for (row, start) in [
        (one_year, "2024-12-31"),
        (three_year, "2022-12-30"),
        (five_year, "2020-12-31"),
    ] {
        let (beta, correlation, tracking_error, information_ratio) =
            aligned_metrics(&subject, &reference, start, "2025-12-31");
        assert_close(available(&row.beta), beta);
        assert_close(available(&row.correlation), correlation);
        assert_close(
            available(&row.tracking_error_annualized_pct),
            tracking_error,
        );
        assert_close(available(&row.information_ratio), information_ratio);
    }
}

fn weekdays(start: &str, count: usize) -> Vec<String> {
    let mut date = NaiveDate::parse_from_str(start, "%Y-%m-%d").unwrap();
    let mut dates = Vec::with_capacity(count);
    while dates.len() < count {
        if !matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
            dates.push(date.format("%Y-%m-%d").to_string());
        }
        date += Duration::days(1);
    }
    dates
}

fn weekdays_between(start: &str, end: &str) -> Vec<String> {
    let mut date = NaiveDate::parse_from_str(start, "%Y-%m-%d").unwrap();
    let end = NaiveDate::parse_from_str(end, "%Y-%m-%d").unwrap();
    let mut dates = Vec::new();
    while date <= end {
        if !matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun) {
            dates.push(date.format("%Y-%m-%d").to_string());
        }
        date += Duration::days(1);
    }
    dates
}

fn growing_series(dates: &[String], start: f64, rate: f64) -> PriceSeries {
    dates
        .iter()
        .enumerate()
        .map(|(index, date)| (date.clone(), start * (1.0 + rate).powf(index as f64)))
        .collect()
}

fn varying_growth(dates: &[String]) -> (PriceSeries, PriceSeries) {
    let mut subject = vec![(dates[0].clone(), 100.0)];
    let mut reference = vec![(dates[0].clone(), 50.0)];
    for index in 0..dates.len() - 1 {
        let subject_return = 0.0004 + 0.0002 * (index % 7) as f64;
        let reference_return = 0.0003 + 0.0001 * (index % 5) as f64;
        subject.push((
            dates[index + 1].clone(),
            subject.last().unwrap().1 * (1.0 + subject_return),
        ));
        reference.push((
            dates[index + 1].clone(),
            reference.last().unwrap().1 * (1.0 + reference_return),
        ));
    }
    (subject, reference)
}

fn standard_period(
    periods: &[FundBenchmarkPeriod],
    period: BenchmarkComparisonPeriod,
) -> &FundBenchmarkPeriod {
    periods
        .iter()
        .find(|candidate| candidate.period == period)
        .expect("the standard period should be reported")
}

fn aligned_metrics(
    subject: &[(String, f64)],
    reference: &[(String, f64)],
    start: &str,
    end: &str,
) -> (f64, f64, f64, f64) {
    let subject_values: std::collections::BTreeMap<&str, f64> = subject
        .iter()
        .map(|(date, value)| (date.as_str(), *value))
        .collect();
    let reference_values: std::collections::BTreeMap<&str, f64> = reference
        .iter()
        .map(|(date, value)| (date.as_str(), *value))
        .collect();
    let dates: Vec<&str> = subject_values
        .range(start..=end)
        .map(|(date, _)| *date)
        .filter(|date| reference_values.contains_key(*date))
        .collect();
    let subject_returns: Vec<f64> = dates
        .windows(2)
        .map(|window| (subject_values[window[1]] / subject_values[window[0]]).ln())
        .collect();
    let reference_returns: Vec<f64> = dates
        .windows(2)
        .map(|window| (reference_values[window[1]] / reference_values[window[0]]).ln())
        .collect();
    let active_returns: Vec<f64> = dates
        .windows(2)
        .map(|window| {
            (subject_values[window[1]] / subject_values[window[0]] - 1.0)
                - (reference_values[window[1]] / reference_values[window[0]] - 1.0)
        })
        .collect();
    let covariance = sample_covariance(&subject_returns, &reference_returns);
    let beta = covariance / sample_variance(&reference_returns);
    let correlation = covariance
        / (sample_variance(&subject_returns) * sample_variance(&reference_returns)).sqrt();
    let active_mean = active_returns.iter().sum::<f64>() / active_returns.len() as f64;
    let active_std = sample_variance(&active_returns).sqrt();
    let tracking_error = active_std * 252.0_f64.sqrt() * 100.0;
    let information_ratio = active_mean / active_std * 252.0_f64.sqrt();
    (beta, correlation, tracking_error, information_ratio)
}

fn available(fact: &ComparisonFact<f64>) -> f64 {
    match fact {
        ComparisonFact::Available { value } => *value,
        other => panic!("expected available fact, got {other:?}"),
    }
}

fn sample_variance(values: &[f64]) -> f64 {
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64
}

fn sample_covariance(left: &[f64], right: &[f64]) -> f64 {
    let left_mean = left.iter().sum::<f64>() / left.len() as f64;
    let right_mean = right.iter().sum::<f64>() / right.len() as f64;
    left.iter()
        .zip(right)
        .map(|(left, right)| (left - left_mean) * (right - right_mean))
        .sum::<f64>()
        / (left.len() - 1) as f64
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
}
