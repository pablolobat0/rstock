use rstock::services::benchmark::compare_aligned_eur_series;

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
        compare_aligned_eur_series(&fund, &reference).unwrap();

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
    assert!(compare_aligned_eur_series(&fund, &reference)
        .unwrap_err()
        .contains("internal weekday observation missing"));
}
