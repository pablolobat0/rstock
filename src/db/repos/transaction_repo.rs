use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::db::entities::transaction;
use crate::models::{
    f64_to_cents, BuyOrder, DividendOrder, SellOrder, SplitOrder, Transaction, TxType,
};

const BULK_WRITE_SIZE: usize = 100;

pub enum TransactionWrite {
    Buy { asset_id: i32, order: BuyOrder },
    Sell { asset_id: i32, order: SellOrder },
    Dividend { asset_id: i32, order: DividendOrder },
    Split { asset_id: i32, order: SplitOrder },
}

pub async fn insert_buy(
    db: &impl ConnectionTrait,
    asset_id: i32,
    order: &BuyOrder,
) -> anyhow::Result<i32> {
    let tx = buy_active_model(asset_id, order);
    let result = tx.insert(db).await?;
    Ok(result.id)
}

pub async fn find_all_ordered_by_date(
    db: &impl ConnectionTrait,
    start_date: Option<&str>,
    end_date: Option<&str>,
) -> anyhow::Result<Vec<Transaction>> {
    let mut query = transaction::Entity::find()
        .order_by_asc(transaction::Column::Date)
        .order_by_asc(transaction::Column::Id);

    if let Some(start) = start_date {
        query = query.filter(transaction::Column::Date.gte(start.to_string()));
    }
    if let Some(end) = end_date {
        query = query.filter(transaction::Column::Date.lte(end.to_string()));
    }

    let results = query.all(db).await?;
    Ok(results.into_iter().map(Transaction::from).collect())
}

pub async fn find_by_asset_id(
    db: &impl ConnectionTrait,
    asset_id: i32,
) -> anyhow::Result<Vec<Transaction>> {
    let results = transaction::Entity::find()
        .filter(transaction::Column::AssetId.eq(asset_id))
        .all(db)
        .await?;
    Ok(results.into_iter().map(Transaction::from).collect())
}

pub async fn insert_sell(
    db: &impl ConnectionTrait,
    asset_id: i32,
    order: &SellOrder,
) -> anyhow::Result<i32> {
    let tx = sell_active_model(asset_id, order);
    let result = tx.insert(db).await?;
    Ok(result.id)
}

pub async fn insert_dividend(
    db: &impl ConnectionTrait,
    asset_id: i32,
    order: &DividendOrder,
) -> anyhow::Result<i32> {
    let tx = dividend_active_model(asset_id, order);
    let result = tx.insert(db).await?;
    Ok(result.id)
}

pub async fn insert_split(
    db: &impl ConnectionTrait,
    asset_id: i32,
    order: &SplitOrder,
) -> anyhow::Result<i32> {
    let tx = split_active_model(asset_id, order);
    let result = tx.insert(db).await?;
    Ok(result.id)
}

pub async fn insert_many(
    db: &impl ConnectionTrait,
    transactions: &[TransactionWrite],
) -> anyhow::Result<()> {
    for chunk in transactions.chunks(BULK_WRITE_SIZE) {
        let models = chunk.iter().map(active_model_for_write);
        transaction::Entity::insert_many(models).exec(db).await?;
    }
    Ok(())
}

pub async fn find_by_id(db: &impl ConnectionTrait, id: i32) -> anyhow::Result<Option<Transaction>> {
    let result = transaction::Entity::find_by_id(id).one(db).await?;
    Ok(result.map(Transaction::from))
}

pub async fn delete_by_id(db: &impl ConnectionTrait, id: i32) -> anyhow::Result<()> {
    transaction::Entity::delete_by_id(id).exec(db).await?;
    Ok(())
}

pub async fn update_by_id(
    db: &impl ConnectionTrait,
    id: i32,
    date: Option<String>,
    quantity: Option<f64>,
    price_cents: Option<i64>,
    fees_cents: Option<i64>,
) -> anyhow::Result<()> {
    let record = transaction::Entity::find_by_id(id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;

    let mut active: transaction::ActiveModel = record.into();
    if let Some(d) = date {
        active.date = Set(d);
    }
    if let Some(q) = quantity {
        active.quantity = Set(q);
    }
    if let Some(p) = price_cents {
        active.price_cents = Set(p);
    }
    if let Some(f) = fees_cents {
        active.fees_cents = Set(f);
    }
    active.update(db).await?;
    Ok(())
}

fn active_model_for_write(write: &TransactionWrite) -> transaction::ActiveModel {
    match write {
        TransactionWrite::Buy { asset_id, order } => buy_active_model(*asset_id, order),
        TransactionWrite::Sell { asset_id, order } => sell_active_model(*asset_id, order),
        TransactionWrite::Dividend { asset_id, order } => dividend_active_model(*asset_id, order),
        TransactionWrite::Split { asset_id, order } => split_active_model(*asset_id, order),
    }
}

fn buy_active_model(asset_id: i32, order: &BuyOrder) -> transaction::ActiveModel {
    active_model(
        asset_id,
        &TxType::Buy,
        &order.date,
        order.quantity,
        f64_to_cents(order.price),
        f64_to_cents(order.fees),
    )
}

fn sell_active_model(asset_id: i32, order: &SellOrder) -> transaction::ActiveModel {
    active_model(
        asset_id,
        &TxType::Sell,
        &order.date,
        order.quantity,
        f64_to_cents(order.price),
        f64_to_cents(order.fees),
    )
}

fn dividend_active_model(asset_id: i32, order: &DividendOrder) -> transaction::ActiveModel {
    active_model(
        asset_id,
        &TxType::Dividend,
        &order.date,
        1.0,
        f64_to_cents(order.amount),
        f64_to_cents(order.fees),
    )
}

fn split_active_model(asset_id: i32, order: &SplitOrder) -> transaction::ActiveModel {
    active_model(asset_id, &TxType::Split, &order.date, order.ratio, 0, 0)
}

fn active_model(
    asset_id: i32,
    tx_type: &TxType,
    date: &str,
    quantity: f64,
    price_cents: i64,
    fees_cents: i64,
) -> transaction::ActiveModel {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set(tx_type.to_string()),
        date: Set(date.to_owned()),
        quantity: Set(quantity),
        price_cents: Set(price_cents),
        fees_cents: Set(fees_cents),
        created_at: Set(now),
        ..Default::default()
    }
}
