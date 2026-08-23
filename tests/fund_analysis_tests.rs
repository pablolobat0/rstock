mod common;

use chrono::Duration;
use common::{
    insert_asset, insert_fund_asset, insert_portfolio_asset_snapshot, insert_portfolio_snapshot,
    insert_transaction, market_data, market_data_at, setup_test_db, MockMarketDataSources,
};
use rstock::constants::format_date;
use rstock::constants::BENCHMARK_TICKER;
use rstock::db::entities::fund_holdings_snapshot;
use rstock::db::repos::{fund_holdings_snapshot_repo, portfolio_history_repo};
use rstock::models::{
    CandidateCorrelationPeriod, FundComparisonPeriod, FundData, FundHolding, FundQuoteMetadata,
    HoldingChangeType,
};
use rstock::services::fund_analysis::{
    compute_breakdown, compute_fingerprint, compute_fund_analysis, compute_holding_diff,
    compute_top_n_weight,
};
use rstock::services::fund_comparison::{compare_funds, compute_common_holdings};
use rstock::services::metrics::compute_cagr;
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::{json, Value};

fn fund_analysis_envelope(result: &rstock::models::FundAnalysisResult) -> Value {
    let mut output = Vec::new();
    rstock::cli::output::write_json(&mut output, "analyze.fund", result)
        .expect("fund analysis JSON should serialize");
    let text = String::from_utf8(output).expect("fund analysis JSON should be UTF-8");
    assert_eq!(text.lines().count(), 1);
    assert!(!text.contains("\u{1b}["));
    serde_json::from_str(&text).expect("fund analysis output should be valid JSON")
}

fn fund_comparison_envelope(result: &rstock::models::FundComparisonResult) -> Value {
    let mut output = Vec::new();
    rstock::cli::output::write_json(&mut output, "compare.funds", result)
        .expect("fund comparison JSON should serialize");
    let text = String::from_utf8(output).expect("fund comparison JSON should be UTF-8");
    assert_eq!(text.lines().count(), 1);
    assert!(!text.contains("\u{1b}["));
    serde_json::from_str(&text).expect("fund comparison output should be valid JSON")
}

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
fn test_common_holdings_match_by_ticker() {
    let holdings_a = vec![holding_with_ticker("Apple Inc", 5.0, Some("AAPL"))];
    let holdings_b = vec![holding_with_ticker("Apple", 3.0, Some("AAPL"))];

    let matches = compute_common_holdings(&holdings_a, &holdings_b);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].ticker.as_deref(), Some("AAPL"));
    assert_eq!(matches[0].name_a, "Apple Inc");
}

#[test]
fn test_common_holdings_match_by_normalized_name_without_ticker() {
    let holdings_a = vec![holding_with_ticker("  Private   Holding ", 2.0, None)];
    let holdings_b = vec![holding_with_ticker("private holding", 1.5, None)];

    let matches = compute_common_holdings(&holdings_a, &holdings_b);

    assert_eq!(matches.len(), 1);
    assert!(matches[0].ticker.is_none());
}

#[test]
fn test_common_holdings_exclude_cash_holdings() {
    let holdings_a = vec![
        holding_with_ticker("Cash", 3.0, None),
        holding_with_ticker("Cash", 2.0, Some("CASH")),
        holding_with_ticker("Apple Inc", 5.0, Some("AAPL")),
    ];
    let holdings_b = vec![
        holding_with_ticker("Cash", 4.0, None),
        holding_with_ticker("Cash", 1.0, Some("CASH")),
        holding_with_ticker("Apple", 3.0, Some("AAPL")),
    ];

    let matches = compute_common_holdings(&holdings_a, &holdings_b);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].ticker.as_deref(), Some("AAPL"));
}

#[test]
fn test_common_holdings_do_not_fuzzy_match_similar_names() {
    let holdings_a = vec![holding_with_ticker("Apple Inc", 5.0, None)];
    let holdings_b = vec![holding_with_ticker("Apple Incorporated", 3.0, None)];

    let matches = compute_common_holdings(&holdings_a, &holdings_b);

    assert!(matches.is_empty());
}

#[test]
fn test_common_holdings_do_not_match_same_name_with_different_tickers() {
    let holdings_a = vec![holding_with_ticker("Apple", 5.0, Some("AAPL"))];
    let holdings_b = vec![holding_with_ticker("Apple", 3.0, Some("APPL"))];

    let matches = compute_common_holdings(&holdings_a, &holdings_b);

    assert!(matches.is_empty());
}

#[test]
fn test_common_holdings_sort_by_larger_fund_weight() {
    let holdings_a = vec![
        holding_with_ticker("Low", 1.0, Some("LOW")),
        holding_with_ticker("High", 3.0, Some("HIGH")),
    ];
    let holdings_b = vec![
        holding_with_ticker("High B", 2.0, Some("HIGH")),
        holding_with_ticker("Low B", 9.0, Some("LOW")),
    ];

    let matches = compute_common_holdings(&holdings_a, &holdings_b);

    assert_eq!(matches[0].ticker.as_deref(), Some("LOW"));
    assert_eq!(matches[1].ticker.as_deref(), Some("HIGH"));
}

#[tokio::test]
async fn test_compare_funds_computes_selected_period_correlation_and_graph_points() {
    let db = setup_test_db().await;
    let today = chrono::Local::now().date_naive();
    let dates = date_range(today - Duration::days(30), today);
    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XCOMPARE1".to_owned(), fund_data());
    sources
        .fund_data
        .insert("XCOMPARE2".to_owned(), fund_data());
    sources
        .historical_prices
        .insert("XCOMPARE1".to_owned(), linear_prices(&dates, 100.0, 1.0));
    sources
        .historical_prices
        .insert("XCOMPARE2".to_owned(), linear_prices(&dates, 200.0, 2.0));
    let market_data = market_data(&sources);

    let result = compare_funds(
        &db,
        &market_data,
        "XCOMPARE1",
        "XCOMPARE2",
        FundComparisonPeriod {
            label: "30D",
            days: 30,
        },
    )
    .await
    .expect("fund comparison should compute");

    assert_eq!(result.correlation.period_label, "30D");
    assert!(result.correlation.correlation.is_some());
    assert!(result.correlation.reason.is_none());
    assert_eq!(result.correlation.points.len(), dates.len());
    assert_eq!(result.correlation.points[0].return_a, 0.0);
    assert_eq!(result.correlation.points[0].return_b, 0.0);
    let last = result.correlation.points.last().unwrap();
    assert!((last.return_a - 30.0).abs() < 0.01);
    assert!((last.return_b - 30.0).abs() < 0.01);
}

#[tokio::test]
async fn test_compare_funds_requires_full_selected_period_coverage() {
    let db = setup_test_db().await;
    let today = chrono::Local::now().date_naive();
    let dates = date_range(today - Duration::days(5), today);
    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert("XSHORT1".to_owned(), fund_data());
    sources.fund_data.insert("XSHORT2".to_owned(), fund_data());
    sources
        .historical_prices
        .insert("XSHORT1".to_owned(), linear_prices(&dates, 100.0, 1.0));
    sources
        .historical_prices
        .insert("XSHORT2".to_owned(), linear_prices(&dates, 200.0, 2.0));
    let market_data = market_data(&sources);

    let result = compare_funds(
        &db,
        &market_data,
        "XSHORT1",
        "XSHORT2",
        FundComparisonPeriod {
            label: "30D",
            days: 30,
        },
    )
    .await
    .expect("fund comparison should compute without fallback graph");

    assert!(result.correlation.correlation.is_none());
    assert!(result.correlation.points.is_empty());
    assert_eq!(
        result.correlation.reason.as_deref(),
        Some("first fund lacks selected-period start coverage")
    );
}

#[tokio::test]
async fn test_fund_comparison_json_complete_structure_and_aligned_points() {
    let db = setup_test_db().await;
    let today = chrono::Local::now().date_naive();
    let dates = date_range(today - Duration::days(2_000), today);
    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XJSONCMP1".to_owned(), fund_data());
    sources.fund_data.insert(
        "XJSONCMP2".to_owned(),
        FundData {
            fund_currency: Some("USD".to_owned()),
            total_holdings: Some(2),
            portfolio_date: Some("2025-02-28".to_owned()),
            holdings: vec![
                fund_holding("Apple", 3.5, Some("Technology"), Some("US"), Some("USD")),
                fund_holding("Toyota", 2.5, Some("Consumer"), Some("Japan"), Some("JPY")),
            ],
        },
    );
    sources.fund_quote_metadata.insert(
        "XJSONCMP1".to_owned(),
        FundQuoteMetadata {
            name: Some("First Candidate".to_owned()),
            aum: Some(1_000_000.0),
            aum_currency: Some("EUR".to_owned()),
            inception_date: Some("2010-01-02".to_owned()),
            quote_currency: Some("EUR".to_owned()),
        },
    );
    sources.fund_quote_metadata.insert(
        "XJSONCMP2".to_owned(),
        FundQuoteMetadata {
            name: Some("Second Candidate".to_owned()),
            aum: Some(2_000_000.0),
            aum_currency: Some("USD".to_owned()),
            inception_date: Some("2012-03-04".to_owned()),
            quote_currency: Some("USD".to_owned()),
        },
    );
    sources
        .historical_prices
        .insert("XJSONCMP1".to_owned(), linear_prices(&dates, 100.0, 0.05));
    sources
        .historical_prices
        .insert("XJSONCMP2".to_owned(), linear_prices(&dates, 200.0, 0.04));
    sources.historical_prices.insert(
        BENCHMARK_TICKER.to_owned(),
        linear_prices(&dates, 300.0, 0.03),
    );

    let result = compare_funds(
        &db,
        &market_data(&sources),
        "XJSONCMP1",
        "XJSONCMP2",
        thirty_day_fund_comparison_period(),
    )
    .await
    .unwrap();
    let envelope = fund_comparison_envelope(&result);

    assert_eq!(envelope["command"], "compare.funds");
    let data = &envelope["data"];
    assert_eq!(data["fund_a"]["code"], "XJSONCMP1");
    assert_eq!(data["fund_a"]["name"], "First Candidate");
    assert_eq!(data["fund_a"]["info"]["aum"], json!(1_000_000.0));
    assert_eq!(data["fund_a"]["info"]["aum_currency"], "EUR");
    assert_eq!(data["fund_a"]["info"]["inception_date"], "2010-01-02");
    assert_eq!(data["fund_a"]["info"]["total_holdings"], 2);
    assert_eq!(data["fund_a"]["info"]["top_10_weight"], json!(10.0));
    assert_eq!(data["fund_a"]["info"]["portfolio_date"], "2025-01-31");
    assert_eq!(data["fund_b"]["code"], "XJSONCMP2");
    assert_eq!(data["fund_b"]["name"], "Second Candidate");
    assert_eq!(data["fund_b"]["info"]["currency"], "USD");
    for fund in ["fund_a", "fund_b"] {
        for period in ["ytd", "one_year", "three_year", "five_year", "all_time"] {
            assert!(
                data[fund][period].is_object(),
                "{fund}.{period} should contain metrics"
            );
            for metric in [
                "total_return",
                "cagr",
                "volatility",
                "sharpe",
                "sortino",
                "max_drawdown",
                "beta",
            ] {
                assert!(
                    data[fund][period].get(metric).is_some(),
                    "{fund}.{period}.{metric} should be present"
                );
            }
        }
    }
    for section in [
        "sector_allocations",
        "country_allocations",
        "currency_allocations",
    ] {
        assert!(!data[section].as_array().unwrap().is_empty());
        let allocation = &data[section][0];
        assert!(allocation["label"].is_string());
        assert!(allocation["weight_a"].is_number());
        assert!(allocation["weight_b"].is_number());
    }
    let apple = data["common_holdings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|holding| holding["ticker"] == "Appl")
        .expect("Apple should be a Common fund holding");
    assert_eq!(apple["weight_a"], json!(6.0));
    assert_eq!(apple["weight_b"], json!(3.5));
    assert_eq!(data["correlation"]["period_label"], "30D");
    assert!(data["correlation"]["correlation"].is_number());
    assert!(data["correlation"]["reason"].is_null());
    let point = &data["correlation"]["points"][0];
    assert!(point["date"].is_string());
    assert!(point["return_a"].is_number());
    assert!(point["return_b"].is_number());
}

#[tokio::test]
async fn test_fund_comparison_json_missing_coverage_uses_nulls_and_empty_points() {
    let db = setup_test_db().await;
    let today = chrono::Local::now().date_naive();
    let dates = date_range(today - Duration::days(5), today);
    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XJSONSHORT1".to_owned(), fund_data());
    sources
        .fund_data
        .insert("XJSONSHORT2".to_owned(), fund_data());
    sources
        .historical_prices
        .insert("XJSONSHORT1".to_owned(), linear_prices(&dates, 100.0, 1.0));
    sources
        .historical_prices
        .insert("XJSONSHORT2".to_owned(), linear_prices(&dates, 200.0, 2.0));

    let result = compare_funds(
        &db,
        &market_data(&sources),
        "XJSONSHORT1",
        "XJSONSHORT2",
        thirty_day_fund_comparison_period(),
    )
    .await
    .unwrap();
    let data = fund_comparison_envelope(&result)["data"].clone();

    assert!(data["fund_a"]["info"]["aum"].is_null());
    assert!(data["fund_b"]["info"]["inception_date"].is_null());
    assert!(data["correlation"]["correlation"].is_null());
    assert_eq!(
        data["correlation"]["reason"],
        "first fund lacks selected-period start coverage"
    );
    assert_eq!(data["correlation"]["points"], json!([]));
}

#[tokio::test]
async fn test_fund_comparison_json_empty_common_holdings_remains_array() {
    let db = setup_test_db().await;
    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert(
        "XJSONUNIQUE1".to_owned(),
        FundData {
            holdings: vec![holding_with_ticker("Unique A", 4.0, Some("XA"))],
            ..fund_data()
        },
    );
    sources.fund_data.insert(
        "XJSONUNIQUE2".to_owned(),
        FundData {
            holdings: vec![holding_with_ticker("Unique B", 5.0, Some("XB"))],
            ..fund_data()
        },
    );
    sources
        .historical_prices
        .insert("XJSONUNIQUE1".to_owned(), Vec::new());
    sources
        .historical_prices
        .insert("XJSONUNIQUE2".to_owned(), Vec::new());

    let result = compare_funds(
        &db,
        &market_data(&sources),
        "XJSONUNIQUE1",
        "XJSONUNIQUE2",
        thirty_day_fund_comparison_period(),
    )
    .await
    .unwrap();
    let data = fund_comparison_envelope(&result)["data"].clone();

    assert_eq!(data["common_holdings"], json!([]));
    assert!(data["common_holdings"].is_array());
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
fn test_compute_cagr_annualizes_short_window() {
    let cagr = compute_cagr("2026-01-01", "2026-04-01", 100.0, 110.0).unwrap();

    assert!(cagr > 0.0);
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
async fn test_fund_analysis_uses_quote_metadata_for_untracked_fund() {
    let db = setup_test_db().await;
    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert("XQUOTE1".to_owned(), fund_data());
    sources.fund_quote_metadata.insert(
        "XQUOTE1".to_owned(),
        FundQuoteMetadata {
            name: Some("Morningstar Fund Name".to_owned()),
            aum: Some(1234567.89),
            aum_currency: Some("USD".to_owned()),
            inception_date: Some("2010-02-03".to_owned()),
            quote_currency: Some("USD".to_owned()),
        },
    );
    sources
        .historical_prices
        .insert("XQUOTE1".to_owned(), fund_prices());

    let result = compute_fund_analysis(
        &db,
        &market_data(&sources),
        "XQUOTE1",
        one_year_candidate_period(),
    )
    .await
    .unwrap();

    assert_eq!(result.name.as_deref(), Some("Morningstar Fund Name"));
    assert_eq!(result.fund_currency.as_deref(), Some("USD"));
    assert_eq!(result.aum, Some(1234567.89));
    assert_eq!(result.aum_currency.as_deref(), Some("USD"));
    assert_eq!(result.inception_date.as_deref(), Some("2010-02-03"));
}

#[tokio::test]
async fn test_fund_analysis_local_name_wins_and_quote_currency_overrides_holdings_currency() {
    let db = setup_test_db().await;
    insert_fund_asset(&db, "XLOCAL1", "Local Fund Name", "EUR", "XQUOTE2").await;
    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert("XQUOTE2".to_owned(), fund_data());
    sources.fund_quote_metadata.insert(
        "XQUOTE2".to_owned(),
        FundQuoteMetadata {
            name: Some("Morningstar Fund Name".to_owned()),
            aum: None,
            aum_currency: None,
            inception_date: None,
            quote_currency: Some("GBP".to_owned()),
        },
    );
    sources
        .historical_prices
        .insert("XQUOTE2".to_owned(), fund_prices());

    let result = compute_fund_analysis(
        &db,
        &market_data(&sources),
        "XQUOTE2",
        one_year_candidate_period(),
    )
    .await
    .unwrap();

    assert_eq!(result.name.as_deref(), Some("Local Fund Name"));
    assert_eq!(result.fund_currency.as_deref(), Some("GBP"));
}

#[tokio::test]
async fn test_fund_analysis_quote_metadata_failure_is_non_fatal() {
    let db = setup_test_db().await;
    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert("XQUOTE3".to_owned(), fund_data());
    sources
        .historical_prices
        .insert("XQUOTE3".to_owned(), fund_prices());

    let result = compute_fund_analysis(
        &db,
        &market_data(&sources),
        "XQUOTE3",
        one_year_candidate_period(),
    )
    .await
    .unwrap();

    assert!(result.name.is_none());
    assert_eq!(result.fund_currency.as_deref(), Some("EUR"));
    assert!(result.aum.is_none());
    assert!(result.inception_date.is_none());
}

#[tokio::test]
async fn test_fund_analysis_candidate_correlation_uses_nav_and_current_holdings() {
    let db = setup_test_db().await;
    let good_asset_id = insert_asset(&db, "XGOOD", "Good Asset", "stock", "EUR").await;
    let missing_asset_id = insert_asset(&db, "XMISS", "Missing Asset", "stock", "EUR").await;

    let end = chrono::Local::now().date_naive() - Duration::days(1);
    let start = end - Duration::days(30);
    let dates = date_range(start, end);
    for (idx, date) in dates.iter().enumerate() {
        insert_portfolio_snapshot(&db, date, 100.0 + idx as f64, 10.0).await;
    }
    let end_str = format_date(end);
    insert_transaction(&db, good_asset_id, &end_str, 1.0, 100.0, 0.0).await;
    insert_transaction(&db, missing_asset_id, &end_str, 1.0, 100.0, 0.0).await;
    insert_portfolio_asset_snapshot(&db, &end_str, good_asset_id, 1.0, 100.0, 100.0, 1.0).await;
    insert_portfolio_asset_snapshot(&db, &end_str, missing_asset_id, 1.0, 100.0, 100.0, 1.0).await;

    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert("XCORR1".to_owned(), fund_data());
    sources
        .historical_prices
        .insert("XCORR1".to_owned(), linear_prices(&dates, 100.0, 1.0));
    sources
        .historical_prices
        .insert("XGOOD".to_owned(), linear_prices(&dates, 50.0, 0.5));

    let result = compute_fund_analysis(
        &db,
        &market_data(&sources),
        "XCORR1",
        CandidateCorrelationPeriod {
            label: "30D",
            days: 30,
        },
    )
    .await
    .unwrap();

    let rows = &result.candidate_correlation.rows;
    assert_eq!(result.candidate_correlation.period_label, "30D");
    assert_eq!(rows[0].label, "Portfolio NAV");
    assert!(rows[0].correlation.is_some());
    let good_row = rows.iter().find(|row| row.label == "Good Asset").unwrap();
    assert!(good_row.correlation.is_some());
    let missing_row = rows
        .iter()
        .find(|row| row.label == "Missing Asset")
        .unwrap();
    assert!(missing_row.correlation.is_none());
    assert_eq!(
        missing_row.reason.as_deref(),
        Some("asset price history unavailable")
    );
}

#[tokio::test]
async fn test_fund_analysis_ensures_nav_history_before_portfolio_view() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XREADYNAV", "Ready NAV Asset", "stock", "EUR").await;
    let today = chrono::NaiveDate::from_ymd_opt(2025, 6, 10).unwrap();
    let end = today - Duration::days(1);
    let dates = date_range(end - Duration::days(30), end);
    common::insert_transaction(&db, asset_id, &dates[0], 1.0, 100.0, 0.0).await;

    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XREADYCORR".to_owned(), fund_data());
    sources
        .historical_prices
        .insert("XREADYCORR".to_owned(), linear_prices(&dates, 100.0, 1.0));
    sources
        .historical_prices
        .insert("XREADYNAV".to_owned(), linear_prices(&dates, 50.0, 0.5));
    let market_data = market_data_at(&sources, today);

    let result = compute_fund_analysis(
        &db,
        &market_data,
        "XREADYCORR",
        CandidateCorrelationPeriod {
            label: "30D",
            days: 30,
        },
    )
    .await
    .unwrap();

    assert_eq!(
        portfolio_history_repo::find_latest(&db)
            .await
            .unwrap()
            .map(|snapshot| snapshot.date),
        Some(format_date(end))
    );
    assert!(result.candidate_correlation.rows[0].correlation.is_some());
}

#[tokio::test]
async fn test_fund_analysis_json_complete_data() {
    let db = setup_test_db().await;
    let today = chrono::Local::now().date_naive();
    let dates = date_range(today - Duration::days(1_400), today);
    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XJSONFULL".to_owned(), fund_data());
    sources.fund_quote_metadata.insert(
        "XJSONFULL".to_owned(),
        FundQuoteMetadata {
            name: Some("Complete Candidate Fund".to_owned()),
            aum: Some(1_234_567.89),
            aum_currency: Some("USD".to_owned()),
            inception_date: Some("2010-02-03".to_owned()),
            quote_currency: Some("EUR".to_owned()),
        },
    );
    sources
        .historical_prices
        .insert("XJSONFULL".to_owned(), linear_prices(&dates, 100.0, 0.05));
    sources.historical_prices.insert(
        BENCHMARK_TICKER.to_owned(),
        linear_prices(&dates, 200.0, 0.03),
    );

    let result = compute_fund_analysis(
        &db,
        &market_data(&sources),
        "XJSONFULL",
        one_year_candidate_period(),
    )
    .await
    .unwrap();
    let envelope = fund_analysis_envelope(&result);

    assert_eq!(envelope["command"], "analyze.fund");
    let data = &envelope["data"];
    assert_eq!(data["ms_code"], "XJSONFULL");
    assert_eq!(data["name"], "Complete Candidate Fund");
    assert_eq!(data["fund_currency"], "EUR");
    assert_eq!(data["aum"], json!(1_234_567.89));
    assert_eq!(data["aum_currency"], "USD");
    assert_eq!(data["inception_date"], "2010-02-03");
    assert_eq!(data["top_holdings"].as_array().unwrap().len(), 2);
    for section in [
        "sector_breakdown",
        "country_breakdown",
        "currency_breakdown",
        "holding_diff",
    ] {
        assert!(data[section].is_array(), "{section} should be an array");
    }
    for period in ["ytd", "one_year", "three_year", "five_year", "all_time"] {
        assert!(data[period].is_object(), "{period} should contain metrics");
        assert!(data[period]["total_return"].is_number());
    }
    assert_eq!(data["candidate_correlation"]["period_label"], "1Y");
    assert!(data["candidate_correlation"]["rows"].is_array());
    assert_eq!(data["holdings_changed"], true);
    assert!(data["last_snapshot_date"].is_null());
}

#[tokio::test]
async fn test_fund_analysis_json_partial_metadata_uses_nulls() {
    let db = setup_test_db().await;
    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert(
        "XJSONPART".to_owned(),
        FundData {
            fund_currency: None,
            total_holdings: None,
            portfolio_date: None,
            holdings: vec![fund_holding_without_ticker(
                "Private Holding",
                2.5,
                None,
                None,
                None,
            )],
        },
    );
    sources.historical_prices.insert(
        "XJSONPART".to_owned(),
        vec![
            ("2025-01-01".to_owned(), 100.0),
            ("2025-01-02".to_owned(), 101.0),
        ],
    );

    let result = compute_fund_analysis(
        &db,
        &market_data(&sources),
        "XJSONPART",
        one_year_candidate_period(),
    )
    .await
    .unwrap();
    let data = fund_analysis_envelope(&result)["data"].clone();

    for field in [
        "name",
        "fund_currency",
        "aum",
        "aum_currency",
        "inception_date",
        "total_holdings",
        "portfolio_date",
    ] {
        assert!(data[field].is_null(), "{field} should be null");
    }
    let holding = &data["top_holdings"][0];
    for field in ["ticker", "sector", "country", "currency"] {
        assert!(holding[field].is_null(), "holding {field} should be null");
    }
    assert_eq!(data["sector_breakdown"], json!([]));
    assert_eq!(data["country_breakdown"], json!([]));
    assert_eq!(data["currency_breakdown"], json!([]));
}

#[tokio::test]
async fn test_fund_analysis_json_unavailable_metrics_are_null() {
    let db = setup_test_db().await;
    let mut sources = MockMarketDataSources::new();
    sources.fund_data.insert(
        "XJSONNONE".to_owned(),
        FundData {
            fund_currency: Some("EUR".to_owned()),
            total_holdings: Some(0),
            portfolio_date: Some("2025-01-31".to_owned()),
            holdings: Vec::new(),
        },
    );
    sources
        .historical_prices
        .insert("XJSONNONE".to_owned(), Vec::new());

    let result = compute_fund_analysis(
        &db,
        &market_data(&sources),
        "XJSONNONE",
        one_year_candidate_period(),
    )
    .await
    .unwrap();
    let data = fund_analysis_envelope(&result)["data"].clone();

    for period in ["ytd", "one_year", "three_year", "five_year", "all_time"] {
        assert!(data[period].is_null(), "{period} should be null");
    }
    assert!(data["top_10_weight"].is_null());
    for section in [
        "top_holdings",
        "sector_breakdown",
        "country_breakdown",
        "currency_breakdown",
        "holding_diff",
    ] {
        assert_eq!(data[section], json!([]), "{section} should be empty");
    }
}

#[tokio::test]
async fn test_fund_analysis_json_correlation_omissions_include_reasons() {
    let db = setup_test_db().await;
    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XJSONCORR".to_owned(), fund_data());
    sources
        .historical_prices
        .insert("XJSONCORR".to_owned(), fund_prices());

    let result = compute_fund_analysis(
        &db,
        &market_data(&sources),
        "XJSONCORR",
        one_year_candidate_period(),
    )
    .await
    .unwrap();
    let rows = fund_analysis_envelope(&result)["data"]["candidate_correlation"]["rows"].clone();

    assert_eq!(rows.as_array().unwrap().len(), 1);
    assert_eq!(rows[0]["label"], "Portfolio NAV");
    assert!(rows[0]["correlation"].is_null());
    assert_eq!(rows[0]["reason"], "portfolio history unavailable");
    assert_eq!(rows[0]["is_portfolio"], true);
}

#[tokio::test]
async fn test_fund_analysis_json_holdings_changes_include_typed_diffs() {
    let db = setup_test_db().await;
    let mut initial_sources = MockMarketDataSources::new();
    initial_sources
        .fund_data
        .insert("XJSONDIFF".to_owned(), fund_data_with_date("2025-01-31"));
    initial_sources
        .historical_prices
        .insert("XJSONDIFF".to_owned(), fund_prices());
    compute_fund_analysis(
        &db,
        &market_data(&initial_sources),
        "XJSONDIFF",
        one_year_candidate_period(),
    )
    .await
    .unwrap();

    let mut changed_sources = MockMarketDataSources::new();
    changed_sources.fund_data.insert(
        "XJSONDIFF".to_owned(),
        FundData {
            portfolio_date: Some("2025-02-28".to_owned()),
            holdings: vec![
                fund_holding("Apple", 7.0, Some("Technology"), Some("US"), Some("USD")),
                fund_holding("NVIDIA", 3.0, Some("Technology"), Some("US"), Some("USD")),
            ],
            ..fund_data()
        },
    );
    changed_sources
        .historical_prices
        .insert("XJSONDIFF".to_owned(), fund_prices());

    let result = compute_fund_analysis(
        &db,
        &market_data(&changed_sources),
        "XJSONDIFF",
        one_year_candidate_period(),
    )
    .await
    .unwrap();
    let data = fund_analysis_envelope(&result)["data"].clone();

    assert_eq!(data["holdings_changed"], true);
    assert_eq!(data["last_snapshot_date"], "2025-01-31");
    let diffs = data["holding_diff"].as_array().unwrap();
    assert_eq!(diffs.len(), 3);
    assert!(diffs.iter().any(|diff| {
        diff["name"] == "NVIDIA"
            && diff["change_type"] == "added"
            && diff["old_weight"].is_null()
            && diff["new_weight"] == 3.0
    }));
    assert!(diffs.iter().any(|diff| {
        diff["name"] == "Microsoft"
            && diff["change_type"] == "removed"
            && diff["old_weight"] == 4.0
            && diff["new_weight"].is_null()
    }));
    assert!(diffs.iter().any(|diff| {
        diff["name"] == "Apple"
            && diff["change_type"] == "weight_changed"
            && diff["old_weight"] == 6.0
            && diff["new_weight"] == 7.0
    }));
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

#[tokio::test]
async fn test_compare_funds_records_snapshots_for_both_funds() {
    let db = setup_test_db().await;
    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XCOMP1".to_owned(), fund_data_with_date("2025-02-01"));
    sources
        .fund_data
        .insert("XCOMP2".to_owned(), fund_data_with_date("2025-03-01"));
    sources
        .historical_prices
        .insert("XCOMP1".to_owned(), fund_prices());
    sources
        .historical_prices
        .insert("XCOMP2".to_owned(), fund_prices());

    compare_funds(
        &db,
        &market_data(&sources),
        "XCOMP1",
        "XCOMP2",
        thirty_day_fund_comparison_period(),
    )
    .await
    .unwrap();

    let snapshot_a =
        fund_holdings_snapshot_repo::find_by_snapshot_date(&db, "XCOMP1", "2025-02-01")
            .await
            .unwrap()
            .expect("fund A snapshot should be recorded");
    let snapshot_b =
        fund_holdings_snapshot_repo::find_by_snapshot_date(&db, "XCOMP2", "2025-03-01")
            .await
            .unwrap()
            .expect("fund B snapshot should be recorded");

    assert_eq!(snapshot_a.total_holdings, Some(2));
    assert_eq!(snapshot_b.total_holdings, Some(2));
    assert_eq!(
        snapshot_a.fingerprint,
        compute_fingerprint(&fund_data().holdings)
    );
    assert_eq!(
        snapshot_b.fingerprint,
        compute_fingerprint(&fund_data().holdings)
    );
}

#[tokio::test]
async fn test_compare_funds_reuses_existing_reported_snapshot() {
    let db = setup_test_db().await;
    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XREUSE1".to_owned(), fund_data_with_date("2025-04-01"));
    sources
        .fund_data
        .insert("XREUSE2".to_owned(), fund_data_with_date("2025-04-02"));
    sources
        .historical_prices
        .insert("XREUSE1".to_owned(), fund_prices());
    sources
        .historical_prices
        .insert("XREUSE2".to_owned(), fund_prices());
    let market_data = market_data(&sources);

    compare_funds(
        &db,
        &market_data,
        "XREUSE1",
        "XREUSE2",
        thirty_day_fund_comparison_period(),
    )
    .await
    .unwrap();
    compare_funds(
        &db,
        &market_data,
        "XREUSE1",
        "XREUSE2",
        thirty_day_fund_comparison_period(),
    )
    .await
    .unwrap();

    assert_eq!(snapshot_count(&db, "XREUSE1", "2025-04-01").await, 1);
    assert_eq!(snapshot_count(&db, "XREUSE2", "2025-04-02").await, 1);
}

#[tokio::test]
async fn test_compare_funds_snapshot_falls_back_to_today_without_portfolio_date() {
    let db = setup_test_db().await;
    let today = format_date(chrono::Local::now().date_naive());
    let mut sources = MockMarketDataSources::new();
    sources
        .fund_data
        .insert("XTODAY1".to_owned(), fund_data_without_portfolio_date());
    sources
        .fund_data
        .insert("XTODAY2".to_owned(), fund_data_with_date("2025-05-01"));
    sources
        .historical_prices
        .insert("XTODAY1".to_owned(), fund_prices());
    sources
        .historical_prices
        .insert("XTODAY2".to_owned(), fund_prices());

    compare_funds(
        &db,
        &market_data(&sources),
        "XTODAY1",
        "XTODAY2",
        thirty_day_fund_comparison_period(),
    )
    .await
    .unwrap();

    assert_eq!(snapshot_count(&db, "XTODAY1", &today).await, 1);
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

fn holding_with_ticker(name: &str, weighting: f64, ticker: Option<&str>) -> FundHolding {
    FundHolding {
        name: name.to_owned(),
        weighting,
        ticker: ticker.map(str::to_owned),
        sector: None,
        country: None,
        currency: None,
    }
}

fn fund_data() -> FundData {
    FundData {
        fund_currency: Some("EUR".to_owned()),
        total_holdings: Some(2),
        portfolio_date: Some("2025-01-31".to_owned()),
        holdings: vec![
            fund_holding("Apple", 6.0, Some("Technology"), Some("US"), Some("USD")),
            fund_holding(
                "Microsoft",
                4.0,
                Some("Technology"),
                Some("US"),
                Some("USD"),
            ),
        ],
    }
}

fn fund_data_with_date(portfolio_date: &str) -> FundData {
    FundData {
        portfolio_date: Some(portfolio_date.to_owned()),
        ..fund_data()
    }
}

fn fund_data_without_portfolio_date() -> FundData {
    FundData {
        portfolio_date: None,
        ..fund_data()
    }
}

fn fund_prices() -> Vec<(String, f64)> {
    vec![
        ("2025-01-01".to_owned(), 100.0),
        ("2025-01-02".to_owned(), 101.0),
        ("2025-01-03".to_owned(), 102.0),
    ]
}

fn one_year_candidate_period() -> CandidateCorrelationPeriod {
    CandidateCorrelationPeriod {
        label: "1Y",
        days: 365,
    }
}

fn thirty_day_fund_comparison_period() -> FundComparisonPeriod {
    FundComparisonPeriod {
        label: "30D",
        days: 30,
    }
}

fn date_range(start: chrono::NaiveDate, end: chrono::NaiveDate) -> Vec<String> {
    let mut dates = Vec::new();
    let mut date = start;
    while date <= end {
        dates.push(format_date(date));
        date += Duration::days(1);
    }
    dates
}

fn linear_prices(dates: &[String], start_price: f64, step: f64) -> Vec<(String, f64)> {
    dates
        .iter()
        .enumerate()
        .map(|(idx, date)| (date.clone(), start_price + idx as f64 * step))
        .collect()
}

async fn snapshot_count(
    db: &sea_orm::DatabaseConnection,
    ms_code: &str,
    snapshot_date: &str,
) -> u64 {
    fund_holdings_snapshot::Entity::find()
        .filter(fund_holdings_snapshot::Column::MsCode.eq(ms_code))
        .filter(fund_holdings_snapshot::Column::SnapshotDate.eq(snapshot_date))
        .count(db)
        .await
        .unwrap()
}
