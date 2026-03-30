use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::db::entities::transaction;
use crate::models::{
    f64_to_cents, BuyOrder, DividendOrder, SellOrder, SplitOrder, Transaction, TxType,
};

pub async fn insert_buy(
    db: &DatabaseConnection,
    asset_id: i32,
    order: &BuyOrder,
) -> anyhow::Result<()> {
    let price_cents = f64_to_cents(order.price);
    let fees_cents = f64_to_cents(order.fees);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let tx = transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set(TxType::Buy.to_string()),
        date: Set(order.date.clone()),
        quantity: Set(order.quantity),
        price_cents: Set(price_cents),
        fees_cents: Set(fees_cents),
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

pub async fn find_by_asset_id(
    db: &DatabaseConnection,
    asset_id: i32,
) -> anyhow::Result<Vec<Transaction>> {
    let results = transaction::Entity::find()
        .filter(transaction::Column::AssetId.eq(asset_id))
        .all(db)
        .await?;
    Ok(results.into_iter().map(Transaction::from).collect())
}

pub async fn insert_sell(
    db: &DatabaseConnection,
    asset_id: i32,
    order: &SellOrder,
) -> anyhow::Result<()> {
    let price_cents = f64_to_cents(order.price);
    let fees_cents = f64_to_cents(order.fees);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let tx = transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set(TxType::Sell.to_string()),
        date: Set(order.date.clone()),
        quantity: Set(order.quantity),
        price_cents: Set(price_cents),
        fees_cents: Set(fees_cents),
        created_at: Set(now),
        ..Default::default()
    };
    tx.insert(db).await?;
    Ok(())
}

pub async fn insert_dividend(
    db: &DatabaseConnection,
    asset_id: i32,
    order: &DividendOrder,
) -> anyhow::Result<()> {
    let amount_cents = f64_to_cents(order.amount);
    let fees_cents = f64_to_cents(order.fees);
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let tx = transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set(TxType::Dividend.to_string()),
        date: Set(order.date.clone()),
        quantity: Set(1.0),
        price_cents: Set(amount_cents),
        fees_cents: Set(fees_cents),
        created_at: Set(now),
        ..Default::default()
    };
    tx.insert(db).await?;
    Ok(())
}

pub async fn insert_split(
    db: &DatabaseConnection,
    asset_id: i32,
    order: &SplitOrder,
) -> anyhow::Result<()> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();

    let tx = transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set(TxType::Split.to_string()),
        date: Set(order.date.clone()),
        quantity: Set(order.ratio),
        price_cents: Set(0),
        fees_cents: Set(0),
        created_at: Set(now),
        ..Default::default()
    };
    tx.insert(db).await?;
    Ok(())
}

pub async fn find_earliest(db: &DatabaseConnection) -> anyhow::Result<Option<Transaction>> {
    let result = transaction::Entity::find()
        .order_by_asc(transaction::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(Transaction::from))
}
