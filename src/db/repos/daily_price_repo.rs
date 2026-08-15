use std::collections::HashMap;

use sea_orm::{
    sea_query::{Expr, OnConflict},
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter,
    QueryOrder, Set, Statement,
};

use crate::db::entities::daily_asset_price;

const BULK_WRITE_SIZE: usize = 100;

pub struct DailyPriceWrite {
    pub asset_id: i32,
    pub date: String,
    pub price: f64,
    pub is_api_failure: bool,
}

#[derive(FromQueryResult)]
struct DatedPrice {
    date: String,
    value: f64,
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

pub async fn find_prices_between_assets(
    db: &impl ConnectionTrait,
    asset_ids: &[i32],
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<HashMap<i32, Vec<(String, f64)>>> {
    if asset_ids.is_empty() {
        return Ok(HashMap::new());
    }

    let results = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.is_in(asset_ids.to_vec()))
        .filter(daily_asset_price::Column::Date.gte(start_date))
        .filter(daily_asset_price::Column::Date.lte(end_date))
        .filter(daily_asset_price::Column::IsApiFailure.eq(false))
        .order_by_asc(daily_asset_price::Column::AssetId)
        .order_by_asc(daily_asset_price::Column::Date)
        .all(db)
        .await?;

    let mut prices = HashMap::new();
    for price in results {
        prices
            .entry(price.asset_id)
            .or_insert_with(Vec::new)
            .push((price.date, price.closing_price));
    }
    Ok(prices)
}

pub async fn find_coverage_with_seed(
    db: &impl ConnectionTrait,
    asset_id: i32,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let rows = DatedPrice::find_by_statement(Statement::from_sql_and_values(
        db.get_database_backend(),
        r"
            SELECT date, closing_price AS value
            FROM daily_asset_prices
            WHERE asset_id = ? AND is_api_failure = FALSE AND date >= ? AND date <= ?
            UNION ALL
            SELECT date, closing_price AS value
            FROM daily_asset_prices
            WHERE asset_id = ? AND is_api_failure = FALSE AND date = (
                SELECT MAX(date)
                FROM daily_asset_prices
                WHERE asset_id = ? AND is_api_failure = FALSE AND date < ?
            )
            ORDER BY date
        ",
        [
            asset_id.into(),
            start_date.into(),
            end_date.into(),
            asset_id.into(),
            asset_id.into(),
            start_date.into(),
        ],
    ))
    .all(db)
    .await?;
    Ok(rows.into_iter().map(|row| (row.date, row.value)).collect())
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

pub async fn insert_many_immutable(
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
        .on_conflict(immutable_conflict())
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

fn immutable_conflict() -> OnConflict {
    OnConflict::columns([
        daily_asset_price::Column::AssetId,
        daily_asset_price::Column::Date,
    ])
    .update_columns([
        daily_asset_price::Column::ClosingPrice,
        daily_asset_price::Column::IsApiFailure,
    ])
    .action_and_where(Expr::col(daily_asset_price::Column::IsApiFailure).eq(true))
    .to_owned()
}
