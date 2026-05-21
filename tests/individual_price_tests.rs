mod common;

use chrono::Duration;
use rstock::constants::format_date;
use rstock::db::repos::asset_repo;
use rstock::services::individual_price;

fn today_string() -> String {
    format_date(chrono::Local::now().date_naive())
}

fn date_string(days_before_yesterday: i64) -> String {
    let yesterday = chrono::Local::now().date_naive() - Duration::days(1);
    format_date(yesterday - Duration::days(days_before_yesterday))
}

#[tokio::test]
async fn test_stock_display_uses_live_quote_without_persisting_it() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKES1", "Live Stock", "stock", "EUR").await;
    let cached_date = date_string(0);
    common::insert_daily_price(&db, asset_id, &cached_date, 100.0, false).await;

    let asset = asset_repo::find_by_ticker(&db, "XFAKES1")
        .await
        .unwrap()
        .unwrap();
    let mut fetcher = common::MockPriceFetcher::new();
    fetcher
        .historical_prices
        .insert("XFAKES1".to_owned(), vec![(today_string(), 125.0)]);

    let result = individual_price::get_asset_display_market_data(
        &db,
        &asset,
        90.0,
        &cached_date,
        1.0,
        &fetcher,
    )
    .await
    .unwrap();

    assert!((result.native_price - 125.0).abs() < 0.01);
    assert_eq!(result.price_date, today_string());
    assert!(result.limitations.is_empty());
}

#[tokio::test]
async fn test_display_uses_live_fx_quote_for_non_base_currency_asset() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKES2", "USD Stock", "stock", "USD").await;
    let cached_date = date_string(0);
    common::insert_daily_price(&db, asset_id, &cached_date, 100.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", &cached_date, 0.80).await;

    let asset = asset_repo::find_by_ticker(&db, "XFAKES2")
        .await
        .unwrap()
        .unwrap();
    let mut fetcher = common::MockPriceFetcher::new();
    fetcher
        .exchange_rates
        .insert("USDEUR".to_owned(), vec![(today_string(), 0.90)]);

    let result = individual_price::get_asset_display_market_data(
        &db,
        &asset,
        90.0,
        &cached_date,
        0.70,
        &fetcher,
    )
    .await
    .unwrap();

    assert!((result.native_price - 100.0).abs() < 0.01);
    assert!((result.fx_rate - 0.90).abs() < 0.01);
    assert!((result.base_currency_price - 90.0).abs() < 0.01);
    assert!(result.limitations.is_empty());
}

#[tokio::test]
async fn test_fund_display_does_not_use_live_quote() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_fund_asset(&db, "XFAKEF1", "Fund", "EUR", "F000FAKE").await;
    let cached_date = date_string(0);
    common::insert_daily_price(&db, asset_id, &cached_date, 100.0, false).await;

    let asset = asset_repo::find_by_ticker(&db, "XFAKEF1")
        .await
        .unwrap()
        .unwrap();
    let mut fetcher = common::MockPriceFetcher::new();
    fetcher
        .historical_prices
        .insert("F000FAKE".to_owned(), vec![(today_string(), 125.0)]);

    let result = individual_price::get_asset_display_market_data(
        &db,
        &asset,
        90.0,
        &cached_date,
        1.0,
        &fetcher,
    )
    .await
    .unwrap();

    assert!((result.native_price - 100.0).abs() < 0.01);
    assert_eq!(result.price_date, cached_date);
}

#[tokio::test]
async fn test_snapshot_fallback_preserves_display_when_current_data_is_missing() {
    let db = common::setup_test_db().await;
    common::insert_asset(&db, "XFAKES3", "Fallback Stock", "stock", "USD").await;
    let fallback_date = "2025-01-02";

    let asset = asset_repo::find_by_ticker(&db, "XFAKES3")
        .await
        .unwrap()
        .unwrap();

    let result = individual_price::get_asset_display_market_data(
        &db,
        &asset,
        88.0,
        fallback_date,
        0.77,
        &common::MockPriceFetcher::new(),
    )
    .await
    .unwrap();

    assert!((result.native_price - 88.0).abs() < 0.01);
    assert_eq!(result.price_date, fallback_date);
    assert!((result.fx_rate - 0.77).abs() < 0.01);
    assert!((result.base_currency_price - 67.76).abs() < 0.01);
}

#[tokio::test]
async fn test_live_stock_with_stale_cached_fx_returns_actionable_limitation() {
    let db = common::setup_test_db().await;
    let stale_fx_date = date_string(8);
    common::insert_asset(&db, "XFAKES4", "USD Live Stock", "stock", "USD").await;
    common::insert_exchange_rate(&db, "USD", "EUR", &stale_fx_date, 0.80).await;

    let asset = asset_repo::find_by_ticker(&db, "XFAKES4")
        .await
        .unwrap()
        .unwrap();
    let mut fetcher = common::MockPriceFetcher::new();
    fetcher
        .historical_prices
        .insert("XFAKES4".to_owned(), vec![(today_string(), 120.0)]);

    let result = individual_price::get_asset_display_market_data(
        &db,
        &asset,
        90.0,
        &stale_fx_date,
        0.70,
        &fetcher,
    )
    .await
    .unwrap();

    assert!((result.native_price - 120.0).abs() < 0.01);
    assert_eq!(result.limitations.len(), 1);
    assert_eq!(
        result.limitations[0].subject,
        rstock::models::MarketDataSubject::FxRate {
            currency: "USD".to_owned(),
        }
    );
}
