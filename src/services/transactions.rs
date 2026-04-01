use sea_orm::DatabaseConnection;

use crate::constants::{display_date, FLOAT_EPSILON};
use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
    transaction_repo,
};
use crate::models::{AssetInfo, BuyOrder, DividendOrder, SellOrder, SplitOrder, Transaction};

pub async fn buy(db: &DatabaseConnection, asset: AssetInfo, order: BuyOrder) -> anyhow::Result<()> {
    let total = order.quantity * order.price + order.fees;
    let summary = format!(
        "Bought {} units of {} ({}) at {:.2} {} on {}. Total: {:.2} {}",
        order.quantity,
        asset.name,
        asset.ticker,
        order.price,
        asset.currency,
        display_date(&order.date),
        total,
        asset.currency
    );

    let asset_id = asset_repo::get_or_create(db, &asset).await?;

    let order_date = order.date.clone();
    transaction_repo::insert_buy(db, asset_id, &order).await?;

    // Invalidate snapshots from the buy date
    portfolio_history_repo::delete_from_date(db, &order_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &order_date, asset_id).await?;

    println!("{summary}");

    Ok(())
}

pub async fn sell(db: &DatabaseConnection, ticker: String, order: SellOrder) -> anyhow::Result<()> {
    let asset = asset_repo::find_by_ticker(db, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    // Validate holdings at the sell date (accounts for splits)
    let transactions = transaction_repo::find_by_asset_id(db, asset.id).await?;
    let filtered_transactions: Vec<_> = transactions
        .into_iter()
        .filter(|t| t.date <= order.date)
        .collect();
    let net_qty = Transaction::compute_holdings(&filtered_transactions);

    if order.quantity > net_qty + FLOAT_EPSILON {
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
        order.quantity,
        asset.name,
        asset.ticker,
        order.price,
        display_date(&order.date),
        proceeds
    );

    let order_date = order.date.clone();
    transaction_repo::insert_sell(db, asset.id, &order).await?;

    // Invalidate snapshots from the sell date
    portfolio_history_repo::delete_from_date(db, &order_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &order_date, asset.id).await?;

    println!("{summary}");

    Ok(())
}

pub async fn dividend(
    db: &DatabaseConnection,
    ticker: String,
    order: DividendOrder,
) -> anyhow::Result<()> {
    let asset = asset_repo::find_by_ticker(db, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    // Validate holdings at dividend date (accounts for splits)
    let transactions = transaction_repo::find_by_asset_id(db, asset.id).await?;
    let filtered_transactions: Vec<_> = transactions
        .into_iter()
        .filter(|t| t.date <= order.date)
        .collect();
    let net_qty = Transaction::compute_holdings(&filtered_transactions);

    if net_qty <= FLOAT_EPSILON {
        anyhow::bail!(
            "No holdings of {} at date {}",
            ticker,
            display_date(&order.date)
        );
    }

    let net_amount = order.amount - order.fees;
    let summary = format!(
        "Dividend for {} ({}): {:.2} (fees: {:.2}, net: {:.2}) on {}",
        asset.name,
        ticker,
        order.amount,
        order.fees,
        net_amount,
        display_date(&order.date)
    );

    let order_date = order.date.clone();
    transaction_repo::insert_dividend(db, asset.id, &order).await?;

    // Invalidate snapshots from the dividend date
    portfolio_history_repo::delete_from_date(db, &order_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &order_date, asset.id).await?;

    println!("{summary}");

    Ok(())
}

pub async fn split(
    db: &DatabaseConnection,
    ticker: String,
    order: SplitOrder,
) -> anyhow::Result<()> {
    let asset = asset_repo::find_by_ticker(db, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    if order.ratio <= 0.0 {
        anyhow::bail!("Split ratio must be positive, got {}", order.ratio);
    }

    // Validate holdings at split date (accounts for prior splits)
    let transactions = transaction_repo::find_by_asset_id(db, asset.id).await?;
    let earliest_date = transactions
        .first()
        .map_or_else(|| order.date.clone(), |t| t.date.clone());
    let filtered_transactions: Vec<_> = transactions
        .into_iter()
        .filter(|t| t.date <= order.date)
        .collect();
    let net_qty = Transaction::compute_holdings(&filtered_transactions);

    if net_qty <= FLOAT_EPSILON {
        anyhow::bail!(
            "No holdings of {} at date {}",
            ticker,
            display_date(&order.date)
        );
    }

    let post_split_qty = net_qty * order.ratio;
    let summary = format!(
        "Split {} ({}): ratio {}, holdings {:.4} -> {:.4} on {}",
        asset.name,
        ticker,
        order.ratio,
        net_qty,
        post_split_qty,
        display_date(&order.date)
    );

    transaction_repo::insert_split(db, asset.id, &order).await?;

    // Price providers retroactively adjust all historical prices after a split,
    // so the entire price cache for this asset is stale.
    daily_price_repo::delete_all_for_asset(db, asset.id).await?;

    // Invalidate portfolio snapshots from the asset's first transaction,
    // since adjusted prices affect the entire history for this asset.
    portfolio_history_repo::delete_from_date(db, &earliest_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &earliest_date, asset.id).await?;

    println!("{summary}");

    Ok(())
}
