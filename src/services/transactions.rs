use sea_orm::DatabaseConnection;

use crate::db::repos::{
    asset_repo, portfolio_asset_history_repo, portfolio_history_repo, transaction_repo,
};
use crate::models::{AssetInfo, BuyOrder, SellOrder};

pub async fn buy(
    db: &DatabaseConnection,
    asset: AssetInfo,
    order: BuyOrder,
) -> anyhow::Result<()> {
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

    let asset_id = asset_repo::get_or_create(db, &asset).await?;

    let order_date = order.date.clone();
    transaction_repo::insert_buy(db, asset_id, &order).await?;

    // Invalidate snapshots from the buy date
    portfolio_history_repo::delete_from_date(db, &order_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &order_date, asset_id).await?;

    println!("{}", summary);

    Ok(())
}

pub async fn sell(
    db: &DatabaseConnection,
    ticker: String,
    order: SellOrder,
) -> anyhow::Result<()> {
    let asset = asset_repo::find_by_ticker(db, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{}' not found", ticker))?;

    // Validate holdings at the sell date
    let transactions = transaction_repo::find_by_asset_id(db, asset.id).await?;
    let net_qty: f64 = transactions
        .iter()
        .filter(|t| t.date <= order.date)
        .map(|t| {
            if t.tx_type == "sell" {
                -t.quantity
            } else {
                t.quantity
            }
        })
        .sum();

    if order.quantity > net_qty + 1e-9 {
        anyhow::bail!(
            "Insufficient holdings: you have {:.4} units of {} but tried to sell {:.4}",
            net_qty,
            ticker,
            order.quantity
        );
    }

    let proceeds = order.quantity * order.price - order.fees;
    let summary = format!(
        "Sold {} units of {} ({}) at {:.2} on {}. Proceeds: {:.2}",
        order.quantity, asset.name, asset.ticker, order.price, order.date, proceeds
    );

    let order_date = order.date.clone();
    transaction_repo::insert_sell(db, asset.id, &order).await?;

    // Invalidate snapshots from the sell date
    portfolio_history_repo::delete_from_date(db, &order_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &order_date, asset.id).await?;

    println!("{}", summary);

    Ok(())
}
