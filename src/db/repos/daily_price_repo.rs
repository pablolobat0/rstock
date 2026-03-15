use sea_orm::*;

use crate::db::entities::daily_asset_price;

pub async fn find_price(db: &DatabaseConnection, asset_id: i32, date: &str) -> anyhow::Result<Option<f64>> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.eq(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .one(db)
        .await?;
    Ok(result.map(|r| r.closing_price))
}

pub async fn find_price_at_or_before(db: &DatabaseConnection, asset_id: i32, date: &str) -> anyhow::Result<Option<f64>> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.lte(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_desc(daily_asset_price::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| r.closing_price))
}

pub async fn find_price_and_date_at_or_before(db: &DatabaseConnection, asset_id: i32, date: &str) -> anyhow::Result<Option<(f64, String)>> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.lte(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_desc(daily_asset_price::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| (r.closing_price, r.date)))
}

pub async fn find_price_before(db: &DatabaseConnection, asset_id: i32, date: &str) -> anyhow::Result<Option<f64>> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.lt(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_desc(daily_asset_price::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| r.closing_price))
}

pub async fn find_prices_between(
    db: &DatabaseConnection,
    asset_id: i32,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let results = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.gte(start_date))
        .filter(daily_asset_price::Column::Date.lte(end_date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_asc(daily_asset_price::Column::Date)
        .all(db)
        .await?;
    Ok(results
        .into_iter()
        .map(|r| (r.date, r.closing_price))
        .collect())
}

pub async fn exists(db: &DatabaseConnection, asset_id: i32, date: &str) -> anyhow::Result<bool> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.eq(date))
        .one(db)
        .await?;
    Ok(result.is_some())
}

pub async fn upsert(db: &DatabaseConnection, asset_id: i32, date: &str, price: f64, is_api_failure: bool) -> anyhow::Result<()> {
    let existing = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.eq(date))
        .one(db)
        .await?;

    if let Some(record) = existing {
        let mut active: daily_asset_price::ActiveModel = record.into();
        active.closing_price = Set(price);
        active.is_api_failure = Set(is_api_failure);
        active.update(db).await?;
    } else {
        let record = daily_asset_price::ActiveModel {
            asset_id: Set(asset_id),
            date: Set(date.to_owned()),
            closing_price: Set(price),
            is_api_failure: Set(is_api_failure),
            ..Default::default()
        };
        record.insert(db).await?;
    }

    Ok(())
}
