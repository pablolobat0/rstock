mod common;

use common::setup_test_db;
use rstock::db::repos::fund_holdings_snapshot_repo;
use rstock::models::{FundHolding, HoldingChangeType};
use rstock::services::fund_analysis::{
    compute_breakdown, compute_fingerprint, compute_holding_diff, compute_top_n_weight,
};
use rstock::services::metrics::compute_cagr;

#[test]
fn test_sector_breakdown_aggregation_uses_unclassified() {
    let holdings = vec![
        fund_holding("Apple", 5.0, Some("Technology"), Some("US"), Some("USD")),
        fund_holding(
            "Microsoft",
            4.0,
            Some("Technology"),
            Some("US"),
            Some("USD"),
        ),
        fund_holding("JPMorgan", 3.0, Some("Financials"), Some("US"), Some("USD")),
        fund_holding("Toyota", 2.0, Some("Consumer"), Some("Japan"), Some("JPY")),
        fund_holding("NoSector", 1.0, None, Some("UK"), Some("GBP")),
    ];

    let breakdown = compute_breakdown(&holdings, |h| h.sector.clone());

    assert_eq!(breakdown.len(), 4);
    assert_eq!(breakdown[0].label, "Technology");
    assert!((breakdown[0].weight - 60.0).abs() < 0.1);
    assert_eq!(breakdown[1].label, "Financials");
    assert!((breakdown[1].weight - 20.0).abs() < 0.1);
    assert_eq!(breakdown[2].label, "Consumer");
    assert!((breakdown[2].weight - 13.33).abs() < 0.1);
    assert_eq!(breakdown[3].label, "Unclassified");
    assert!((breakdown[3].weight - 6.67).abs() < 0.1);
}

#[test]
fn test_country_breakdown_aggregation() {
    let holdings = vec![
        fund_holding(
            "Apple",
            5.0,
            Some("Tech"),
            Some("United States"),
            Some("USD"),
        ),
        fund_holding(
            "Microsoft",
            3.0,
            Some("Tech"),
            Some("United States"),
            Some("USD"),
        ),
        fund_holding("Toyota", 2.0, Some("Auto"), Some("Japan"), Some("JPY")),
    ];

    let breakdown = compute_breakdown(&holdings, |h| h.country.clone());

    assert_eq!(breakdown.len(), 2);
    assert_eq!(breakdown[0].label, "United States");
    assert!((breakdown[0].weight - 80.0).abs() < 0.1);
    assert_eq!(breakdown[1].label, "Japan");
    assert!((breakdown[1].weight - 20.0).abs() < 0.1);
}

#[test]
fn test_currency_breakdown_aggregation() {
    let holdings = vec![
        fund_holding("Apple", 5.0, Some("Tech"), Some("US"), Some("USD")),
        fund_holding("Toyota", 3.0, Some("Auto"), Some("Japan"), Some("JPY")),
        fund_holding("SAP", 2.0, Some("Tech"), Some("Germany"), Some("EUR")),
    ];

    let breakdown = compute_breakdown(&holdings, |h| h.currency.clone());

    assert_eq!(breakdown.len(), 3);
    assert_eq!(breakdown[0].label, "USD");
    assert!((breakdown[0].weight - 50.0).abs() < 0.1);
    assert_eq!(breakdown[1].label, "JPY");
    assert!((breakdown[1].weight - 30.0).abs() < 0.1);
    assert_eq!(breakdown[2].label, "EUR");
    assert!((breakdown[2].weight - 20.0).abs() < 0.1);
}

#[test]
fn test_breakdown_treats_blank_fields_as_unclassified() {
    let holdings = vec![
        fund_holding("Apple", 7.0, Some("Technology"), Some("US"), Some("USD")),
        fund_holding("Mystery", 3.0, Some(""), Some("US"), Some("USD")),
    ];

    let breakdown = compute_breakdown(&holdings, |h| h.sector.clone());

    assert_eq!(breakdown.len(), 2);
    assert_eq!(breakdown[0].label, "Technology");
    assert!((breakdown[0].weight - 70.0).abs() < 0.1);
    assert_eq!(breakdown[1].label, "Unclassified");
    assert!((breakdown[1].weight - 30.0).abs() < 0.1);
}

#[test]
fn test_breakdown_excludes_non_equity_when_filtered_by_ticker() {
    let holdings = vec![
        fund_holding("Apple", 6.0, Some("Technology"), Some("US"), Some("USD")),
        fund_holding("Toyota", 4.0, Some("Auto"), Some("Japan"), Some("JPY")),
        fund_holding_without_ticker("Cash", 3.0, None, None, None),
        fund_holding_without_ticker("Gov Bond", 2.0, None, None, None),
    ];

    let equity_holdings: Vec<_> = holdings
        .into_iter()
        .filter(|holding| holding.ticker.is_some())
        .collect();
    let breakdown = compute_breakdown(&equity_holdings, |h| h.sector.clone());

    assert_eq!(breakdown.len(), 2);
    assert_eq!(breakdown[0].label, "Technology");
    assert!((breakdown[0].weight - 60.0).abs() < 0.1);
    assert_eq!(breakdown[1].label, "Auto");
    assert!((breakdown[1].weight - 40.0).abs() < 0.1);
}

#[test]
fn test_compute_top_10_weight_uses_largest_holdings() {
    let holdings = vec![
        fund_holding("Small", 1.0, None, None, None),
        fund_holding("Largest", 10.0, None, None, None),
        fund_holding("Second", 9.0, None, None, None),
        fund_holding("Third", 8.0, None, None, None),
        fund_holding("Fourth", 7.0, None, None, None),
        fund_holding("Fifth", 6.0, None, None, None),
        fund_holding("Sixth", 5.0, None, None, None),
        fund_holding("Seventh", 4.0, None, None, None),
        fund_holding("Eighth", 3.0, None, None, None),
        fund_holding("Ninth", 2.0, None, None, None),
        fund_holding("Tenth", 1.5, None, None, None),
    ];

    let top_10_weight = compute_top_n_weight(&holdings, 10).unwrap();

    assert!((top_10_weight - 55.5).abs() < 0.01);
}

#[test]
fn test_compute_top_10_weight_none_for_no_holdings() {
    assert!(compute_top_n_weight(&[], 10).is_none());
}

#[test]
fn test_compute_cagr_positive() {
    let cagr = compute_cagr("2023-01-01", "2026-01-01", 100.0, 200.0).unwrap();
    assert!((cagr - 25.99).abs() < 0.2);
}

#[test]
fn test_compute_cagr_zero_growth() {
    let cagr = compute_cagr("2023-01-01", "2026-01-01", 100.0, 100.0).unwrap();
    assert!(cagr.abs() < 0.01);
}

#[test]
fn test_compute_cagr_negative() {
    let cagr = compute_cagr("2023-01-01", "2026-01-01", 100.0, 70.0).unwrap();
    assert!(cagr < 0.0);
}

#[test]
fn test_compute_cagr_none_for_short_window() {
    assert!(compute_cagr("2026-01-01", "2026-04-01", 100.0, 110.0).is_none());
}

#[test]
fn test_holding_diff_added_removed_changed() {
    let old_json = serde_json::to_string(&vec![
        serde_json::json!({"name": "Apple", "weighting": 5.0}),
        serde_json::json!({"name": "Microsoft", "weighting": 4.0}),
        serde_json::json!({"name": "Meta", "weighting": 3.0}),
    ])
    .unwrap();

    let new_holdings = vec![
        fund_holding("Apple", 5.5, None, None, None),
        fund_holding("Microsoft", 4.0, None, None, None),
        fund_holding("NVIDIA", 3.5, None, None, None),
    ];

    let diff = compute_holding_diff(&old_json, &new_holdings).unwrap();

    let added: Vec<_> = diff
        .iter()
        .filter(|c| matches!(c.change_type, HoldingChangeType::Added))
        .collect();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].name, "NVIDIA");
    assert_eq!(added[0].new_weight, Some(3.5));

    let removed: Vec<_> = diff
        .iter()
        .filter(|c| matches!(c.change_type, HoldingChangeType::Removed))
        .collect();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].name, "Meta");
    assert_eq!(removed[0].old_weight, Some(3.0));

    let changed: Vec<_> = diff
        .iter()
        .filter(|c| matches!(c.change_type, HoldingChangeType::WeightChanged))
        .collect();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].name, "Apple");
    assert_eq!(changed[0].old_weight, Some(5.0));
    assert_eq!(changed[0].new_weight, Some(5.5));
}

#[test]
fn test_holding_diff_no_change_within_tolerance() {
    let old_json = serde_json::to_string(&vec![
        serde_json::json!({"name": "Apple", "weighting": 5.0}),
    ])
    .unwrap();

    let new_holdings = vec![fund_holding("Apple", 5.005, None, None, None)];

    let diff = compute_holding_diff(&old_json, &new_holdings).unwrap();
    assert!(diff.is_empty());
}

#[test]
fn test_fingerprint_deterministic() {
    let holdings_a = vec![
        fund_holding("Apple", 5.0, None, None, None),
        fund_holding("Microsoft", 4.0, None, None, None),
        fund_holding("Google", 3.0, None, None, None),
    ];

    let holdings_b = vec![
        fund_holding("Google", 3.0, None, None, None),
        fund_holding("Apple", 5.0, None, None, None),
        fund_holding("Microsoft", 4.0, None, None, None),
    ];

    let fp_a = compute_fingerprint(&holdings_a);
    let fp_b = compute_fingerprint(&holdings_b);
    assert_eq!(fp_a, fp_b);
}

#[test]
fn test_fingerprint_changes_with_different_weights() {
    let holdings_a = vec![fund_holding("Apple", 5.0, None, None, None)];
    let holdings_b = vec![fund_holding("Apple", 6.0, None, None, None)];

    let fp_a = compute_fingerprint(&holdings_a);
    let fp_b = compute_fingerprint(&holdings_b);
    assert_ne!(fp_a, fp_b);
}

#[tokio::test]
async fn test_snapshot_insert_and_find_latest() {
    let db = setup_test_db().await;

    let result = fund_holdings_snapshot_repo::find_latest(&db, "XTEST1")
        .await
        .unwrap();
    assert!(result.is_none());

    fund_holdings_snapshot_repo::insert(&db, "XTEST1", "2025-01-01", "fp1", "[]", Some(50))
        .await
        .unwrap();

    fund_holdings_snapshot_repo::insert(
        &db,
        "XTEST1",
        "2025-06-01",
        "fp2",
        "[{\"name\": \"A\"}]",
        Some(60),
    )
    .await
    .unwrap();

    // Different code
    fund_holdings_snapshot_repo::insert(&db, "XTEST2", "2025-12-01", "fp3", "[]", None)
        .await
        .unwrap();

    let latest = fund_holdings_snapshot_repo::find_latest(&db, "XTEST1")
        .await
        .unwrap()
        .expect("should find snapshot");
    assert_eq!(latest.ms_code, "XTEST1");
    assert_eq!(latest.snapshot_date, "2025-06-01");
    assert_eq!(latest.fingerprint, "fp2");
    assert_eq!(latest.total_holdings, Some(60));
}

#[tokio::test]
async fn test_snapshot_find_by_snapshot_date() {
    let db = setup_test_db().await;

    fund_holdings_snapshot_repo::insert(&db, "XTEST1", "2025-01-01", "fp1", "[]", Some(50))
        .await
        .unwrap();
    fund_holdings_snapshot_repo::insert(&db, "XTEST1", "2025-06-01", "fp2", "[]", Some(60))
        .await
        .unwrap();

    let snapshot = fund_holdings_snapshot_repo::find_by_snapshot_date(&db, "XTEST1", "2025-01-01")
        .await
        .unwrap()
        .expect("should find snapshot for requested date");
    assert_eq!(snapshot.fingerprint, "fp1");

    let missing = fund_holdings_snapshot_repo::find_by_snapshot_date(&db, "XTEST1", "2025-02-01")
        .await
        .unwrap();
    assert!(missing.is_none());
}

// --- Test helpers ---

fn fund_holding(
    name: &str,
    weighting: f64,
    sector: Option<&str>,
    country: Option<&str>,
    currency: Option<&str>,
) -> FundHolding {
    FundHolding {
        name: name.to_owned(),
        weighting,
        ticker: Some(name.chars().take(4).collect()),
        sector: sector.map(str::to_owned),
        country: country.map(str::to_owned),
        currency: currency.map(str::to_owned),
    }
}

fn fund_holding_without_ticker(
    name: &str,
    weighting: f64,
    sector: Option<&str>,
    country: Option<&str>,
    currency: Option<&str>,
) -> FundHolding {
    FundHolding {
        name: name.to_owned(),
        weighting,
        ticker: None,
        sector: sector.map(str::to_owned),
        country: country.map(str::to_owned),
        currency: currency.map(str::to_owned),
    }
}
