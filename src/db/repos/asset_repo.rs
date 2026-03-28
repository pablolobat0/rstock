use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::db::entities::asset;
use crate::models::{Asset, AssetInfo};

pub async fn find_by_ticker(
    db: &DatabaseConnection,
    ticker: &str,
) -> anyhow::Result<Option<Asset>> {
    let result = asset::Entity::find()
        .filter(asset::Column::Ticker.eq(ticker))
        .one(db)
        .await?;
    Ok(result.map(Asset::from))
}

pub async fn find_by_ids(
    db: &DatabaseConnection,
    ids: impl IntoIterator<Item = i32>,
) -> anyhow::Result<Vec<Asset>> {
    let ids: Vec<i32> = ids.into_iter().collect();
    let results = asset::Entity::find()
        .filter(asset::Column::Id.is_in(ids))
        .all(db)
        .await?;
    Ok(results.into_iter().map(Asset::from).collect())
}

pub async fn find_all(db: &DatabaseConnection) -> anyhow::Result<Vec<Asset>> {
    let results = asset::Entity::find()
        .order_by_asc(asset::Column::Ticker)
        .all(db)
        .await?;
    Ok(results.into_iter().map(Asset::from).collect())
}

pub async fn create(db: &DatabaseConnection, info: &AssetInfo) -> anyhow::Result<i32> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let new_asset = asset::ActiveModel {
        ticker: Set(info.ticker.clone()),
        isin: Set(info.isin.clone()),
        name: Set(info.name.clone()),
        asset_type: Set(info.asset_type.to_string()),
        currency: Set(info.currency.clone()),
        created_at: Set(now),
        ..Default::default()
    };
    let result = new_asset.insert(db).await?;
    Ok(result.id)
}

pub async fn get_or_create(db: &DatabaseConnection, info: &AssetInfo) -> anyhow::Result<i32> {
    if let Some(existing) = find_by_ticker(db, &info.ticker).await? {
        return Ok(existing.id);
    }
    create(db, info).await
}
