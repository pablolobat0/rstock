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
