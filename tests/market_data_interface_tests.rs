mod common;

use chrono::NaiveDate;
use rstock::services::market_data::MarketData;

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
