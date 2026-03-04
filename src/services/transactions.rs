use anyhow::Context;
use sea_orm::*;

use crate::db::entities::{asset, portfolio_asset_history, portfolio_history, transaction};
use crate::models::{AssetInfo, BuyOrder};

pub async fn buy(db: &DatabaseConnection, asset: AssetInfo, order: BuyOrder) -> anyhow::Result<()> {
    let total = order.quantity * order.price + order.fees;
    let summary = format!(
        "Bought {} units of {} ({}) at {:.2} {} on {}. Total: {:.2} {}",
        order.quantity,
        asset.name,
        asset.ticker,
        order.price,
        asset.currency,
        order.date,
        total,
        asset.currency
    );

    let asset_id = get_or_create_asset(db, asset).await?;

    let price_cents = (order.price * 100.0).round() as i64;
    let fees_cents = (order.fees * 100.0).round() as i64;
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let order_date = order.date.clone();

    let tx = transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set("buy".to_owned()),
        date: Set(order.date),
        quantity: Set(order.quantity),
        price_cents: Set(price_cents),
        fees_cents: Set(fees_cents),
        notes: Set(order.notes),
        created_at: Set(now),
        ..Default::default()
    };

    tx.insert(db).await?;

    // Invalidate snapshots from the buy date
    portfolio_history::Entity::delete_many()
        .filter(portfolio_history::Column::Date.gte(&order_date))
        .exec(db)
        .await
        .context("failed to delete stale portfolio_history")?;
    portfolio_asset_history::Entity::delete_many()
        .filter(portfolio_asset_history::Column::Date.gte(&order_date))
        .filter(portfolio_asset_history::Column::AssetId.eq(asset_id))
        .exec(db)
        .await
        .context("failed to delete stale portfolio_asset_history")?;

    println!("{}", summary);

    Ok(())
}

async fn get_or_create_asset(db: &DatabaseConnection, asset: AssetInfo) -> anyhow::Result<i32> {
    if let Some(existing) = asset::Entity::find()
        .filter(asset::Column::Ticker.eq(&asset.ticker))
        .one(db)
        .await?
    {
        return Ok(existing.id);
    }

    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let new_asset = asset::ActiveModel {
        ticker: Set(asset.ticker),
        isin: Set(asset.isin),
        name: Set(asset.name),
        asset_type: Set(asset.asset_type),
        currency: Set(asset.currency),
        created_at: Set(now),
        ..Default::default()
    };

    let result = new_asset.insert(db).await?;
    Ok(result.id)
}
