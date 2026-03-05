use sea_orm::DatabaseConnection;

use crate::db::repos::{
    asset_repo, portfolio_asset_history_repo, portfolio_history_repo, transaction_repo,
};
use crate::models::{AssetInfo, BuyOrder};

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
