use sea_orm::*;

use crate::db::entities::portfolio_asset_history;
use crate::models::AssetSnapshot;

pub async fn find_by_date(db: &DatabaseConnection, date: &str) -> anyhow::Result<Vec<AssetSnapshot>> {
    let results = portfolio_asset_history::Entity::find()
        .filter(portfolio_asset_history::Column::Date.eq(date))
        .all(db)
        .await?;
    Ok(results.into_iter().map(AssetSnapshot::from).collect())
}

pub async fn upsert(db: &DatabaseConnection, snapshot: &AssetSnapshot) -> anyhow::Result<()> {
    let existing = portfolio_asset_history::Entity::find()
        .filter(portfolio_asset_history::Column::Date.eq(&snapshot.date))
        .filter(portfolio_asset_history::Column::AssetId.eq(snapshot.asset_id))
        .one(db)
        .await?;

    if let Some(record) = existing {
        let mut active: portfolio_asset_history::ActiveModel = record.into();
        active.quantity = Set(snapshot.quantity);
        active.closing_price = Set(snapshot.closing_price);
        active.market_value = Set(snapshot.market_value);
        active.exchange_rate = Set(snapshot.exchange_rate);
        active.update(db).await?;
    } else {
        let record = portfolio_asset_history::ActiveModel {
            date: Set(snapshot.date.clone()),
            asset_id: Set(snapshot.asset_id),
            quantity: Set(snapshot.quantity),
            closing_price: Set(snapshot.closing_price),
            market_value: Set(snapshot.market_value),
            exchange_rate: Set(snapshot.exchange_rate),
            ..Default::default()
        };
        record.insert(db).await?;
    }

    Ok(())
}

pub async fn delete_from_date_for_asset(db: &DatabaseConnection, date: &str, asset_id: i32) -> anyhow::Result<()> {
    portfolio_asset_history::Entity::delete_many()
        .filter(portfolio_asset_history::Column::Date.gte(date))
        .filter(portfolio_asset_history::Column::AssetId.eq(asset_id))
        .exec(db)
        .await?;
    Ok(())
}
