#![allow(clippy::float_cmp)]

pub mod common;

use chrono::NaiveDate;
use rstock::services::market_data::MarketData;

#[tokio::test]
async fn market_data_resolves_pence_through_pound_fx_in_both_directions() {
    let mut sources = common::MockMarketDataSources::new();
    sources
        .exchange_rates
        .insert("GBPEUR".to_owned(), vec![("2025-01-02".to_owned(), 1.2)]);
    sources
        .exchange_rates
        .insert("EURGBP".to_owned(), vec![("2025-01-02".to_owned(), 0.8)]);
    let market_data = MarketData::new(Box::new(sources));
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();

    for denomination in ["GBp", "GBX", " gbx "] {
        let rates = market_data
            .exchange_rate_history(denomination, "EUR", start, start)
            .await
            .unwrap();
        assert_eq!(rates.len(), 1, "missing rate for {denomination}");
        assert!(
            (rates[0].value - 0.012).abs() < 1e-12,
            "expected EUR per penny, got {} for {denomination}",
            rates[0].value
        );
        let reverse = market_data
            .exchange_rate_history("EUR", denomination, start, start)
            .await
            .unwrap();
        assert_eq!(reverse.len(), 1);
        assert!((reverse[0].value - 80.0).abs() < 1e-12);
    }

    let pounds = market_data
        .exchange_rate_history("GBP", "EUR", start, start)
        .await
        .unwrap();
    assert_eq!(pounds[0].value, 1.2);
}

#[tokio::test]
async fn market_data_pence_and_pounds_use_fixed_conversion_without_sources() {
    let market_data = MarketData::new(Box::new(common::MockMarketDataSources::new()));
    let start = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    for (from, to, expected) in [
        ("GBX", "GBP", 0.01),
        ("GBP", "GBp", 100.0),
        ("GBp", "GBX", 1.0),
    ] {
        let rates = market_data
            .exchange_rate_history(from, to, start, start)
            .await
            .unwrap();
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].date, start);
        assert_eq!(rates[0].value, expected);
    }
}

#[tokio::test]
async fn market_data_normalizes_fx_currencies_before_source_call() {
    let mut sources = common::MockMarketDataSources::new();
    sources
        .exchange_rates
        .insert("USDEUR".to_owned(), vec![("2025-01-02".to_owned(), 0.92)]);
    let market_data = MarketData::new(Box::new(sources));

    let result = market_data
        .exchange_rate_history(
            "usd",
            "eur",
            NaiveDate::from_ymd_opt(2025, 1, 1).expect("valid date"),
            NaiveDate::from_ymd_opt(2025, 1, 3).expect("valid date"),
        )
        .await
        .expect("exchange rate should load");

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].value, 0.92);
}

#[tokio::test]
async fn market_data_rejects_invalid_fx_currency_before_source_call() {
    let market_data = MarketData::new(Box::new(common::MockMarketDataSources::new()));

    let result = market_data
        .exchange_rate_history(
            "US1",
            "EUR",
            NaiveDate::from_ymd_opt(2025, 1, 1).expect("valid date"),
            NaiveDate::from_ymd_opt(2025, 1, 3).expect("valid date"),
        )
        .await;

    assert!(result.is_err());
}

#[tokio::test]
async fn market_data_same_currency_fx_uses_implicit_rate() {
    let market_data = MarketData::new(Box::new(common::MockMarketDataSources::new()));
    let start = NaiveDate::from_ymd_opt(2025, 1, 1).expect("valid date");

    let result = market_data
        .exchange_rate_history(
            "eur",
            "EUR",
            start,
            NaiveDate::from_ymd_opt(2025, 1, 3).expect("valid date"),
        )
        .await
        .expect("same-currency FX should not call sources");

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].date, start);
    assert_eq!(result[0].value, 1.0);
}
