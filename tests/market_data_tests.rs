mod common;

use chrono::{Duration, Local, NaiveDate};
use rstock::constants::format_date;
use rstock::db::entities::asset;
use rstock::db::repos::{daily_price_repo, exchange_rate_repo};
use rstock::models::Asset;
use rstock::services::market_data;
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

#[tokio::test]
async fn test_nav_market_data_does_not_persist_same_day_asset_or_fx_data() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockPriceFetcher::new();

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

    let effective_end = market_data::prepare_nav_market_data(
        &db,
        std::slice::from_ref(&asset),
        &["USDEUR".to_owned()],
        &yesterday_str,
        &today_str,
        &mock,
    )
    .await
    .unwrap();

    assert_eq!(effective_end, yesterday);
    assert_eq!(
        daily_price_repo::find_price(&db, asset.id, &today_str)
            .await
            .unwrap(),
        None
    );
    assert_eq!(
        exchange_rate_repo::find_rate(&db, "USDEUR", &today_str)
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn test_nav_market_data_persists_asset_forward_fill_between_source_observations_only() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockPriceFetcher::new();

    let asset = make_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    mock.historical_prices.insert(
        "XFAKE1".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 10.0),
            ("2025-01-05".to_owned(), 20.0),
        ],
    );

    let effective_end = market_data::prepare_nav_market_data(
        &db,
        std::slice::from_ref(&asset),
        &[],
        "2025-01-02",
        "2025-01-10",
        &mock,
    )
    .await
    .unwrap();

    assert_eq!(effective_end, NaiveDate::from_ymd_opt(2025, 1, 5).unwrap());
    assert_eq!(
        daily_price_repo::find_price(&db, asset.id, "2025-01-03")
            .await
            .unwrap(),
        Some(10.0)
    );
    assert_eq!(
        daily_price_repo::find_price(&db, asset.id, "2025-01-04")
            .await
            .unwrap(),
        Some(10.0)
    );
    assert_eq!(
        daily_price_repo::find_price(&db, asset.id, "2025-01-06")
            .await
            .unwrap(),
        None
    );
}

#[tokio::test]
async fn test_nav_market_data_persists_fx_forward_fill_between_source_observations_only() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockPriceFetcher::new();

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

    let effective_end = market_data::prepare_nav_market_data(
        &db,
        std::slice::from_ref(&asset),
        &["USDEUR".to_owned()],
        "2025-01-02",
        "2025-01-10",
        &mock,
    )
    .await
    .unwrap();

    assert_eq!(effective_end, NaiveDate::from_ymd_opt(2025, 1, 5).unwrap());
    assert_eq!(
        exchange_rate_repo::find_rate(&db, "USDEUR", "2025-01-03")
            .await
            .unwrap(),
        Some(0.90)
    );
    assert_eq!(
        exchange_rate_repo::find_rate(&db, "USDEUR", "2025-01-04")
            .await
            .unwrap(),
        Some(0.90)
    );
    assert_eq!(
        exchange_rate_repo::find_rate(&db, "USDEUR", "2025-01-06")
            .await
            .unwrap(),
        None
    );
}
