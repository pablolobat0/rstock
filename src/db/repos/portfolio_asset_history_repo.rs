use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait,
    QueryFilter, Set,
};

use crate::db::entities::portfolio_asset_history;
use crate::models::AssetSnapshot;

const BULK_WRITE_SIZE: usize = 100;

pub async fn find_by_date(
    db: &impl ConnectionTrait,
    date: &str,
) -> anyhow::Result<Vec<AssetSnapshot>> {
    let results = portfolio_asset_history::Entity::find()
        .filter(portfolio_asset_history::Column::Date.eq(date))
        .all(db)
        .await?;
    Ok(results.into_iter().map(AssetSnapshot::from).collect())
}

pub async fn upsert(db: &impl ConnectionTrait, snapshot: &AssetSnapshot) -> anyhow::Result<()> {
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

pub async fn upsert_native(
    db: &impl ConnectionTrait,
    snapshot: &AssetSnapshot,
) -> anyhow::Result<()> {
    portfolio_asset_history::Entity::insert(active_model(snapshot))
        .on_conflict(native_conflict())
        .exec_without_returning(db)
        .await?;
    Ok(())
}

pub async fn upsert_many_native(
    db: &impl ConnectionTrait,
    snapshots: &[AssetSnapshot],
) -> anyhow::Result<()> {
    for chunk in snapshots.chunks(BULK_WRITE_SIZE) {
        portfolio_asset_history::Entity::insert_many(chunk.iter().map(active_model))
            .on_conflict(native_conflict())
            .exec_without_returning(db)
            .await?;
    }
    Ok(())
}

pub async fn delete_from_date_for_asset(
    db: &impl ConnectionTrait,
    date: &str,
    asset_id: i32,
) -> anyhow::Result<()> {
    portfolio_asset_history::Entity::delete_many()
        .filter(portfolio_asset_history::Column::Date.gte(date))
        .filter(portfolio_asset_history::Column::AssetId.eq(asset_id))
        .exec(db)
        .await?;
    Ok(())
}

fn active_model(snapshot: &AssetSnapshot) -> portfolio_asset_history::ActiveModel {
    portfolio_asset_history::ActiveModel {
        date: Set(snapshot.date.clone()),
        asset_id: Set(snapshot.asset_id),
        quantity: Set(snapshot.quantity),
        closing_price: Set(snapshot.closing_price),
        market_value: Set(snapshot.market_value),
        exchange_rate: Set(snapshot.exchange_rate),
        ..Default::default()
    }
}

fn native_conflict() -> OnConflict {
    OnConflict::columns([
        portfolio_asset_history::Column::Date,
        portfolio_asset_history::Column::AssetId,
    ])
    .update_columns([
        portfolio_asset_history::Column::Quantity,
        portfolio_asset_history::Column::ClosingPrice,
        portfolio_asset_history::Column::MarketValue,
        portfolio_asset_history::Column::ExchangeRate,
    ])
    .to_owned()
}
