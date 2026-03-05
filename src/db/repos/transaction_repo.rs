use sea_orm::*;

use crate::db::entities::transaction;
use crate::models::{BuyOrder, Transaction};

pub async fn insert_buy(db: &DatabaseConnection, asset_id: i32, order: &BuyOrder) -> anyhow::Result<()> {
    let price_cents = (order.price * 100.0).round() as i64;
    let fees_cents = (order.fees * 100.0).round() as i64;
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let tx = transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set("buy".to_owned()),
        date: Set(order.date.clone()),
        quantity: Set(order.quantity),
        price_cents: Set(price_cents),
        fees_cents: Set(fees_cents),
        notes: Set(order.notes.clone()),
        created_at: Set(now),
        ..Default::default()
    };
    tx.insert(db).await?;
    Ok(())
}

pub async fn find_all_ordered_by_date(db: &DatabaseConnection) -> anyhow::Result<Vec<Transaction>> {
    let results = transaction::Entity::find()
        .order_by_asc(transaction::Column::Date)
        .all(db)
        .await?;
    Ok(results.into_iter().map(Transaction::from).collect())
}

pub async fn find_by_asset_id(db: &DatabaseConnection, asset_id: i32) -> anyhow::Result<Vec<Transaction>> {
    let results = transaction::Entity::find()
        .filter(transaction::Column::AssetId.eq(asset_id))
        .all(db)
        .await?;
    Ok(results.into_iter().map(Transaction::from).collect())
}

pub async fn find_earliest(db: &DatabaseConnection) -> anyhow::Result<Option<Transaction>> {
    let result = transaction::Entity::find()
        .order_by_asc(transaction::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(Transaction::from))
}
