mod common;

use rstock::db::entities::{asset, daily_asset_price};
use rstock::models::Asset;
use sea_orm::{EntityTrait, Set};

async fn make_asset(db: &sea_orm::DatabaseConnection) -> Asset {
    let id = common::insert_asset(db, "TEST", "Test Stock", "stock", "EUR").await;
    let model = asset::Entity::find_by_id(id)
        .one(db)
        .await
        .unwrap()
        .unwrap();
    Asset::from(model)
}

/// Insert price for date → get_closing_price returns it.
#[tokio::test]
async fn test_cached_price_returned() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db).await;

    common::insert_daily_price(&db, asset.id, "2025-01-02", 42.5, false).await;

    let price = common::market_data(&common::MockMarketDataSources::new())
        .get_required_asset_valuation_data(&db, &asset, "2025-01-02")
        .await
        .unwrap()
        .native_price;
    assert_eq!(price, 42.5);
}

/// Insert price for Monday → Tuesday query returns Monday's price.
#[tokio::test]
async fn test_forward_fill_from_previous() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db).await;

    // Monday price only
    common::insert_daily_price(&db, asset.id, "2025-01-06", 55.0, false).await;

    // Tuesday query should forward-fill
    let price = common::market_data(&common::MockMarketDataSources::new())
        .get_required_asset_valuation_data(&db, &asset, "2025-01-07")
        .await
        .unwrap()
        .native_price;
    assert_eq!(price, 55.0);
}

/// Empty table → returns None.
#[tokio::test]
async fn test_no_price_returns_none() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db).await;

    let result = common::market_data(&common::MockMarketDataSources::new())
        .get_required_asset_valuation_data(&db, &asset, "2025-01-02")
        .await;
    assert!(result.is_err());
}

/// Insert is_api_failure=true entry → exact match skipped, falls through to forward-fill.
#[tokio::test]
async fn test_api_failure_entry_skipped() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db).await;

    // Good price on Monday
    common::insert_daily_price(&db, asset.id, "2025-01-06", 50.0, false).await;
    // Failure entry on Tuesday
    common::insert_daily_price(&db, asset.id, "2025-01-07", 0.0, true).await;

    // Query Tuesday — should skip failure, forward-fill from Monday
    let price = common::market_data(&common::MockMarketDataSources::new())
        .get_required_asset_valuation_data(&db, &asset, "2025-01-07")
        .await
        .unwrap()
        .native_price;
    assert_eq!(price, 50.0);
}

/// Prices on day 1 and day 5 → query day 3 returns day 1's price (most recent ≤ target).
#[tokio::test]
async fn test_forward_fill_prefers_recent() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db).await;

    common::insert_daily_price(&db, asset.id, "2025-01-01", 10.0, false).await;
    common::insert_daily_price(&db, asset.id, "2025-01-05", 20.0, false).await;

    let price = common::market_data(&common::MockMarketDataSources::new())
        .get_required_asset_valuation_data(&db, &asset, "2025-01-03")
        .await
        .unwrap()
        .native_price;
    // Should return day 1's price (most recent on or before day 3)
    assert_eq!(price, 10.0);
}

/// Insert failure entry, then upsert with good data → record updated.
#[tokio::test]
async fn test_upsert_overwrites_failure() {
    use sea_orm::ColumnTrait;
    use sea_orm::QueryFilter;

    let db = common::setup_test_db().await;
    let asset = make_asset(&db).await;

    // Insert failure
    common::insert_daily_price(&db, asset.id, "2025-01-02", 0.0, true).await;

    // Verify failure entry exists
    let result = common::market_data(&common::MockMarketDataSources::new())
        .get_required_asset_valuation_data(&db, &asset, "2025-01-02")
        .await;
    assert!(result.is_err()); // Failure entries are skipped

    // Now update the record directly (simulating what upsert_price would do)
    let record = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset.id))
        .filter(daily_asset_price::Column::Date.eq("2025-01-02"))
        .one(&db)
        .await
        .unwrap()
        .unwrap();

    let mut active: daily_asset_price::ActiveModel = record.into();
    active.closing_price = Set(75.0);
    active.is_api_failure = Set(false);
    daily_asset_price::Entity::update(active)
        .exec(&db)
        .await
        .unwrap();

    // Now it should return the good price
    let price = common::market_data(&common::MockMarketDataSources::new())
        .get_required_asset_valuation_data(&db, &asset, "2025-01-02")
        .await
        .unwrap()
        .native_price;
    assert_eq!(price, 75.0);
}
