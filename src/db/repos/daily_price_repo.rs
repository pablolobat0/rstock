use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait,
    QueryFilter, QueryOrder, Set,
};

use crate::db::entities::daily_asset_price;

const BULK_WRITE_SIZE: usize = 100;

pub struct DailyPriceWrite {
    pub asset_id: i32,
    pub date: String,
    pub price: f64,
    pub is_api_failure: bool,
}

pub async fn find_price(
    db: &impl ConnectionTrait,
    asset_id: i32,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.eq(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .one(db)
        .await?;
    Ok(result.map(|r| r.closing_price))
}

pub async fn find_price_at_or_before(
    db: &impl ConnectionTrait,
    asset_id: i32,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.lte(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_desc(daily_asset_price::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| r.closing_price))
}

pub async fn find_price_and_date_at_or_before(
    db: &impl ConnectionTrait,
    asset_id: i32,
    date: &str,
) -> anyhow::Result<Option<(f64, String)>> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.lte(date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_desc(daily_asset_price::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| (r.closing_price, r.date)))
}

pub async fn find_latest_date(
    db: &impl ConnectionTrait,
    asset_id: i32,
) -> anyhow::Result<Option<String>> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_desc(daily_asset_price::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| r.date))
}

pub async fn find_price_before(
    db: &impl ConnectionTrait,
    asset_id: i32,
    date: &str,
) -> anyhow::Result<Option<f64>> {
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
    db: &impl ConnectionTrait,
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

pub async fn exists(db: &impl ConnectionTrait, asset_id: i32, date: &str) -> anyhow::Result<bool> {
    let result = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .filter(daily_asset_price::Column::Date.eq(date))
        .one(db)
        .await?;
    Ok(result.is_some())
}

pub async fn upsert(
    db: &impl ConnectionTrait,
    asset_id: i32,
    date: &str,
    price: f64,
    is_api_failure: bool,
) -> anyhow::Result<()> {
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

pub async fn upsert_native(
    db: &impl ConnectionTrait,
    asset_id: i32,
    date: &str,
    price: f64,
    is_api_failure: bool,
) -> anyhow::Result<()> {
    daily_asset_price::Entity::insert(active_model(asset_id, date, price, is_api_failure))
        .on_conflict(native_conflict())
        .exec_without_returning(db)
        .await?;
    Ok(())
}

pub async fn upsert_many_native(
    db: &impl ConnectionTrait,
    prices: &[DailyPriceWrite],
) -> anyhow::Result<()> {
    for chunk in prices.chunks(BULK_WRITE_SIZE) {
        daily_asset_price::Entity::insert_many(chunk.iter().map(|price| {
            active_model(
                price.asset_id,
                &price.date,
                price.price,
                price.is_api_failure,
            )
        }))
        .on_conflict(native_conflict())
        .exec_without_returning(db)
        .await?;
    }
    Ok(())
}

pub async fn delete_all_for_asset(db: &impl ConnectionTrait, asset_id: i32) -> anyhow::Result<()> {
    daily_asset_price::Entity::delete_many()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .exec(db)
        .await?;
    Ok(())
}

fn active_model(
    asset_id: i32,
    date: &str,
    price: f64,
    is_api_failure: bool,
) -> daily_asset_price::ActiveModel {
    daily_asset_price::ActiveModel {
        asset_id: Set(asset_id),
        date: Set(date.to_owned()),
        closing_price: Set(price),
        is_api_failure: Set(is_api_failure),
        ..Default::default()
    }
}

fn native_conflict() -> OnConflict {
    OnConflict::columns([
        daily_asset_price::Column::AssetId,
        daily_asset_price::Column::Date,
    ])
    .update_columns([
        daily_asset_price::Column::ClosingPrice,
        daily_asset_price::Column::IsApiFailure,
    ])
    .to_owned()
}
