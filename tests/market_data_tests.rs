mod common;

use chrono::{Duration, Local, NaiveDate};
use rstock::constants::{format_date, BENCHMARK_NAME, BENCHMARK_TICKER};
use rstock::db::entities::asset;
use rstock::models::{Asset, MarketDataLimitationClassification, MarketDataSubject};
use sea_orm::EntityTrait;

async fn make_asset(
    db: &sea_orm::DatabaseConnection,
    ticker: &str,
    name: &str,
    asset_type: &str,
    currency: &str,
) -> Asset {
    let id = common::insert_asset(db, ticker, name, asset_type, currency).await;
    let model = asset::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    Asset::from(model)
}

async fn make_fund_asset(
    db: &sea_orm::DatabaseConnection,
    ticker: &str,
    name: &str,
    currency: &str,
    morningstar_code: &str,
) -> Asset {
    let id = common::insert_fund_asset(db, ticker, name, currency, morningstar_code).await;
    let model = asset::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    Asset::from(model)
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

#[tokio::test]
async fn test_benchmark_market_data_prepares_prices_through_market_data() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    mock.historical_prices.insert(
        BENCHMARK_TICKER.to_owned(),
        vec![("2025-01-02".to_owned(), 100.0)],
    );
    mock.exchange_rates
        .insert("USDEUR".to_owned(), vec![("2025-01-02".to_owned(), 0.90)]);

    let benchmark_market_data = common::market_data(&mock)
        .correlation_market_data(&db, Vec::new(), "2025-01-02", "2025-01-02")
        .await
        .unwrap();

    assert_eq!(benchmark_market_data.limitations, vec![]);
    assert_eq!(
        common::find_daily_price(
            &db,
            benchmark_market_data.benchmark_series.asset_id,
            "2025-01-02"
        )
        .await
        .unwrap(),
        Some(100.0)
    );
}

#[tokio::test]
async fn test_benchmark_market_data_prepares_required_fx() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    mock.historical_prices.insert(
        BENCHMARK_TICKER.to_owned(),
        vec![("2025-01-02".to_owned(), 100.0)],
    );
    mock.exchange_rates
        .insert("USDEUR".to_owned(), vec![("2025-01-02".to_owned(), 0.90)]);

    common::market_data(&mock)
        .correlation_market_data(&db, Vec::new(), "2025-01-02", "2025-01-02")
        .await
        .unwrap();

    assert_eq!(
        common::find_exchange_rate(&db, "USD", "EUR", "2025-01-02")
            .await
            .unwrap(),
        Some(0.90)
    );
}

#[tokio::test]
async fn test_benchmark_market_data_stays_distinct_from_holdings() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let held_asset = make_asset(&db, "XFAKE1", "Held Stock", "stock", "EUR").await;
    common::insert_transaction(&db, held_asset.id, "2025-01-02", 1.0, 10.0, 0.0).await;
    mock.historical_prices.insert(
        BENCHMARK_TICKER.to_owned(),
        vec![("2025-01-02".to_owned(), 100.0)],
    );
    mock.exchange_rates
        .insert("USDEUR".to_owned(), vec![("2025-01-02".to_owned(), 0.90)]);

    let benchmark_market_data = common::market_data(&mock)
        .correlation_market_data(&db, Vec::new(), "2025-01-02", "2025-01-02")
        .await
        .unwrap();
    let benchmark = asset::Entity::find_by_id(benchmark_market_data.benchmark_series.asset_id)
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(benchmark.ticker, BENCHMARK_TICKER);
    assert_eq!(benchmark.name, BENCHMARK_NAME);
    assert_ne!(
        benchmark_market_data.benchmark_series.asset_id,
        held_asset.id
    );
    assert!(common::get_asset_snapshots(&db, "2025-01-02")
        .await
        .is_empty());
}

#[tokio::test]
async fn test_nav_market_data_does_not_persist_same_day_asset_or_fx_data() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset = make_asset(&db, "XFAKEUSD", "US Stock", "stock", "USD").await;
    let today = Local::now().date_naive();
    let yesterday = today - Duration::days(1);
    let today_str = format_date(today);
    let yesterday_str = format_date(yesterday);

    mock.historical_prices.insert(
        "XFAKEUSD".to_owned(),
        vec![(yesterday_str.clone(), 100.0), (today_str.clone(), 101.0)],
    );
    mock.exchange_rates.insert(
        "USDEUR".to_owned(),
        vec![(yesterday_str.clone(), 0.90), (today_str.clone(), 0.91)],
    );

    let valuation_market_data = common::market_data(&mock)
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            &yesterday_str,
            &today_str,
        )
        .await
        .unwrap();

    assert_eq!(valuation_market_data.effective_end, yesterday);
    assert_eq!(
        common::find_daily_price(&db, asset.id, &today_str)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        common::find_exchange_rate(&db, "USD", "EUR", &today_str)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn test_nav_market_data_persists_asset_forward_fill_between_source_observations_only() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset = make_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    mock.historical_prices.insert(
        "XFAKE1".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 10.0),
            ("2025-01-05".to_owned(), 20.0),
        ],
    );

    let valuation_market_data = common::market_data(&mock)
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-02",
            "2025-01-10",
        )
        .await
        .unwrap();

    assert_eq!(
        valuation_market_data.effective_end,
        NaiveDate::from_ymd_opt(2025, 1, 5).unwrap()
    );
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-03")
            .await
            .unwrap(),
        Some(10.0)
    );
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-04")
            .await
            .unwrap(),
        Some(10.0)
    );
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-06")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn test_nav_market_data_uses_implicit_base_currency_fx() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset = make_asset(&db, "XFAKEEUR", "EUR Stock", "stock", "EUR").await;
    mock.historical_prices
        .insert("XFAKEEUR".to_owned(), vec![("2025-01-02".to_owned(), 10.0)]);

    let valuation_market_data = common::market_data(&mock)
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-02",
            "2025-01-02",
        )
        .await
        .unwrap();

    assert_eq!(valuation_market_data.effective_end, date(2025, 1, 2));
    assert_eq!(
        common::find_exchange_rate(&db, "EUR", "EUR", "2025-01-02")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn test_nav_market_data_persists_fx_forward_fill_between_source_observations_only() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset = make_asset(&db, "XFAKEUSD", "US Stock", "stock", "USD").await;
    mock.historical_prices.insert(
        "XFAKEUSD".to_owned(),
        vec![("2025-01-05".to_owned(), 100.0)],
    );
    mock.exchange_rates.insert(
        "USDEUR".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 0.90),
            ("2025-01-05".to_owned(), 0.95),
        ],
    );

    let valuation_market_data = common::market_data(&mock)
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-02",
            "2025-01-10",
        )
        .await
        .unwrap();

    assert_eq!(
        valuation_market_data.effective_end,
        NaiveDate::from_ymd_opt(2025, 1, 5).unwrap()
    );
    assert_eq!(
        common::find_exchange_rate(&db, "USD", "EUR", "2025-01-03")
            .await
            .unwrap(),
        Some(0.90)
    );
    assert_eq!(
        common::find_exchange_rate(&db, "USD", "EUR", "2025-01-04")
            .await
            .unwrap(),
        Some(0.90)
    );
    assert_eq!(
        common::find_exchange_rate(&db, "USD", "EUR", "2025-01-06")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn test_nav_market_data_returns_stale_cached_asset_limitation() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset = make_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_daily_price(&db, asset.id, "2025-01-01", 10.0, false).await;

    let valuation_market_data = common::market_data(&mock)
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-07",
            "2025-01-07",
        )
        .await
        .unwrap();

    assert_eq!(
        valuation_market_data.effective_end,
        NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()
    );
    assert_eq!(valuation_market_data.limitations.len(), 1);
    assert_eq!(
        valuation_market_data.limitations[0].classification,
        MarketDataLimitationClassification::ActionableStaleData
    );
}

#[tokio::test]
async fn test_nav_market_data_returns_only_actionable_reporting_lag() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let acceptable = make_fund_asset(&db, "XFAKEF1", "Test Fund 1", "EUR", "FUND1").await;
    let excessive = make_fund_asset(&db, "XFAKEF2", "Test Fund 2", "EUR", "FUND2").await;
    mock.historical_prices
        .insert("FUND1".to_owned(), vec![("2025-01-03".to_owned(), 10.0)]);
    mock.historical_prices
        .insert("FUND2".to_owned(), vec![("2025-01-02".to_owned(), 20.0)]);

    let valuation_market_data = common::market_data(&mock)
        .prepare_valuation_market_data(&db, &[acceptable, excessive], "2025-01-02", "2025-01-10")
        .await
        .unwrap();

    assert_eq!(valuation_market_data.limitations.len(), 1);
    assert_eq!(
        valuation_market_data.limitations[0].classification,
        MarketDataLimitationClassification::ActionableReportingLag
    );
    assert_eq!(
        valuation_market_data.limitations[0].subject,
        MarketDataSubject::Asset {
            ticker: "XFAKEF2".to_owned(),
            name: "Test Fund 2".to_owned(),
            asset_type: rstock::models::AssetType::Fund,
        }
    );
}

#[tokio::test]
async fn test_nav_market_data_stock_stale_limitations_ignore_weekends() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset = make_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    mock.historical_prices
        .insert("XFAKE1".to_owned(), vec![("2025-01-03".to_owned(), 10.0)]);

    let valuation_market_data = common::market_data(&mock)
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-03",
            "2025-01-05",
        )
        .await
        .unwrap();

    assert!(valuation_market_data.limitations.is_empty());
}

#[tokio::test]
async fn test_nav_market_data_returns_fx_completed_weekday_stale_limitation() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset = make_asset(&db, "XFAKEUSD", "US Stock", "stock", "USD").await;
    mock.historical_prices.insert(
        "XFAKEUSD".to_owned(),
        vec![("2025-01-07".to_owned(), 100.0)],
    );
    mock.exchange_rates
        .insert("USDEUR".to_owned(), vec![("2025-01-01".to_owned(), 0.90)]);

    let valuation_market_data = common::market_data(&mock)
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-07",
        )
        .await
        .unwrap();

    assert_eq!(valuation_market_data.effective_end, date(2025, 1, 1));
    assert_eq!(valuation_market_data.limitations.len(), 1);
    assert_eq!(
        valuation_market_data.limitations[0].classification,
        MarketDataLimitationClassification::ActionableStaleData
    );
    assert_eq!(
        valuation_market_data.limitations[0].subject,
        MarketDataSubject::FxRate {
            currency: "USD".to_owned()
        }
    );
}
