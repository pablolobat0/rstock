mod common;

use chrono::{Duration, NaiveDate};
use rstock::constants::format_date;
use rstock::db::repos::asset_repo;
use rstock::models::{
    AssetType, IndividualPriceFallback, MarketDataLimitationClassification, MarketDataSubject,
};

fn fixed_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()
}

fn fixed_fallback(price_date: &str) -> IndividualPriceFallback {
    IndividualPriceFallback {
        native_price: 90.0,
        price_date: price_date.to_owned(),
        fx_rate: 1.0,
    }
}

fn today_string() -> String {
    format_date(chrono::Local::now().date_naive())
}

fn date_string(days_before_yesterday: i64) -> String {
    let yesterday = chrono::Local::now().date_naive() - Duration::days(1);
    format_date(yesterday - Duration::days(days_before_yesterday))
}

#[tokio::test]
async fn fixed_clock_etf_display_uses_capability_based_live_quote() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_etf_asset(&db, "XFAKEETF1", "Live ETF", "EUR", "ETF1").await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 100.0, false).await;

    let asset = asset_repo::find_by_ticker(&db, "XFAKEETF1")
        .await
        .unwrap()
        .unwrap();
    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert("ETF1".to_owned(), vec![("2025-06-10".to_owned(), 125.0)]);
    sources
        .historical_prices
        .insert(asset.ticker.clone(), vec![("2025-06-10".to_owned(), 999.0)]);

    let result = common::market_data_at(&sources, fixed_today())
        .individual_price(&db, &asset, fixed_fallback("2025-06-09"))
        .await
        .unwrap();

    assert!((result.native_price - 125.0).abs() < 1e-9);
    assert_eq!(result.price_date, "2025-06-10");
    assert!(result.limitations.is_empty());
}

#[tokio::test]
async fn fixed_clock_etf_display_falls_back_to_historical_price_with_limitation() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_etf_asset(&db, "XFAKEETF2", "Fallback ETF", "EUR", "ETF2").await;
    common::insert_daily_price(&db, asset_id, "2025-05-30", 101.0, false).await;

    let asset = asset_repo::find_by_ticker(&db, "XFAKEETF2")
        .await
        .unwrap()
        .unwrap();
    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert("ETF2".to_owned(), vec![("2025-06-09".to_owned(), 999.0)]);

    let result = common::market_data_at(&sources, fixed_today())
        .individual_price(&db, &asset, fixed_fallback("2025-05-29"))
        .await
        .unwrap();

    assert!((result.native_price - 101.0).abs() < 1e-9);
    assert_eq!(result.price_date, "2025-05-30");
    assert_eq!(result.limitations.len(), 1);
    assert_eq!(
        result.limitations[0].classification,
        MarketDataLimitationClassification::ActionableReportingLag
    );
    assert_eq!(
        result.limitations[0].subject,
        MarketDataSubject::Asset {
            ticker: asset.ticker.clone(),
            name: asset.name.clone(),
            asset_type: AssetType::Etf,
        }
    );
}

#[tokio::test]
async fn fixed_clock_fund_display_keeps_historical_closing_price_semantics() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_fund_asset(&db, "XFAKEF2", "Fund", "EUR", "FUND2").await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 100.0, false).await;

    let asset = asset_repo::find_by_ticker(&db, "XFAKEF2")
        .await
        .unwrap()
        .unwrap();
    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert(asset.ticker.clone(), vec![("2025-06-10".to_owned(), 125.0)]);
    sources
        .historical_prices
        .insert("FUND2".to_owned(), vec![("2025-06-10".to_owned(), 130.0)]);

    let result = common::market_data_at(&sources, fixed_today())
        .individual_price(&db, &asset, fixed_fallback("2025-06-08"))
        .await
        .unwrap();

    assert!((result.native_price - 100.0).abs() < 1e-9);
    assert_eq!(result.price_date, "2025-06-09");
    assert!(result.limitations.is_empty());
}

#[tokio::test]
async fn fixed_clock_stock_display_still_uses_live_quote() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKES5", "Live Stock", "stock", "EUR").await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 100.0, false).await;

    let asset = asset_repo::find_by_ticker(&db, "XFAKES5")
        .await
        .unwrap()
        .unwrap();
    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert(asset.ticker.clone(), vec![("2025-06-10".to_owned(), 126.0)]);

    let result = common::market_data_at(&sources, fixed_today())
        .individual_price(&db, &asset, fixed_fallback("2025-06-09"))
        .await
        .unwrap();

    assert!((result.native_price - 126.0).abs() < 1e-9);
    assert_eq!(result.price_date, "2025-06-10");
    assert!(result.limitations.is_empty());
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
    let mut fetcher = common::MockMarketDataSources::new();
    fetcher
        .historical_prices
        .insert("XFAKES1".to_owned(), vec![(today_string(), 125.0)]);

    let result = common::market_data(&fetcher)
        .individual_price(
            &db,
            &asset,
            IndividualPriceFallback {
                native_price: 90.0,
                price_date: cached_date.clone(),
                fx_rate: 1.0,
            },
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
    let mut fetcher = common::MockMarketDataSources::new();
    fetcher
        .exchange_rates
        .insert("USDEUR".to_owned(), vec![(today_string(), 0.90)]);

    let result = common::market_data(&fetcher)
        .individual_price(
            &db,
            &asset,
            IndividualPriceFallback {
                native_price: 90.0,
                price_date: cached_date.clone(),
                fx_rate: 0.70,
            },
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
    let mut fetcher = common::MockMarketDataSources::new();
    fetcher
        .historical_prices
        .insert("F000FAKE".to_owned(), vec![(today_string(), 125.0)]);

    let result = common::market_data(&fetcher)
        .individual_price(
            &db,
            &asset,
            IndividualPriceFallback {
                native_price: 90.0,
                price_date: cached_date.clone(),
                fx_rate: 1.0,
            },
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

    let result = common::market_data(&common::MockMarketDataSources::new())
        .individual_price(
            &db,
            &asset,
            IndividualPriceFallback {
                native_price: 88.0,
                price_date: fallback_date.to_owned(),
                fx_rate: 0.77,
            },
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
    let mut fetcher = common::MockMarketDataSources::new();
    fetcher
        .historical_prices
        .insert("XFAKES4".to_owned(), vec![(today_string(), 120.0)]);

    let result = common::market_data(&fetcher)
        .individual_price(
            &db,
            &asset,
            IndividualPriceFallback {
                native_price: 90.0,
                price_date: stale_fx_date.clone(),
                fx_rate: 0.70,
            },
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
