use sea_orm::DatabaseConnection;

use crate::constants::{display_date, FLOAT_EPSILON};
use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
    transaction_repo,
};
use crate::models::{
    f64_to_cents, BuyOrder, DividendOrder, SellOrder, SplitOrder, Transaction, TransactionListItem,
};

#[derive(Debug)]
pub struct TransactionReceipt {
    pub transaction_id: i32,
    pub summary: String,
}

pub async fn list(db: &DatabaseConnection) -> anyhow::Result<Vec<TransactionListItem>> {
    let transactions = transaction_repo::find_all_ordered_by_date(db, None, None).await?;
    let asset_ids = transactions.iter().map(|tx| tx.asset_id);
    let assets = asset_repo::find_by_ids(db, asset_ids).await?;

    let items = transactions
        .into_iter()
        .map(|transaction| {
            let asset = assets.iter().find(|asset| asset.id == transaction.asset_id);
            TransactionListItem {
                transaction,
                ticker: asset.map_or_else(|| "unknown".to_string(), |asset| asset.ticker.clone()),
                asset_name: asset.map_or_else(|| "unknown".to_string(), |asset| asset.name.clone()),
            }
        })
        .collect();

    Ok(items)
}

pub async fn buy(
    db: &DatabaseConnection,
    ticker: String,
    order: BuyOrder,
) -> anyhow::Result<TransactionReceipt> {
    let asset = asset_repo::find_by_ticker(db, &ticker).await?.ok_or_else(|| {
        anyhow::anyhow!(
            "asset with ticker '{ticker}' not found; create it first with `rstock portfolio asset add -t {ticker} ...`"
        )
    })?;

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

    let order_date = order.date.clone();
    let tx_id = transaction_repo::insert_buy(db, asset.id, &order).await?;

    // Invalidate snapshots from the buy date
    portfolio_history_repo::delete_from_date(db, &order_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &order_date, asset.id).await?;

    tracing::info!(
        %ticker,
        quantity = order.quantity,
        price = order.price,
        date = %order.date,
        "buy transaction recorded"
    );
    Ok(TransactionReceipt {
        transaction_id: tx_id,
        summary,
    })
}

pub async fn sell(
    db: &DatabaseConnection,
    ticker: String,
    order: SellOrder,
) -> anyhow::Result<TransactionReceipt> {
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
    let tx_id = transaction_repo::insert_sell(db, asset.id, &order).await?;

    // Invalidate snapshots from the sell date
    portfolio_history_repo::delete_from_date(db, &order_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &order_date, asset.id).await?;

    tracing::info!(
        %ticker,
        quantity = order.quantity,
        price = order.price,
        date = %order.date,
        "sell transaction recorded"
    );
    Ok(TransactionReceipt {
        transaction_id: tx_id,
        summary,
    })
}

pub async fn dividend(
    db: &DatabaseConnection,
    ticker: String,
    order: DividendOrder,
) -> anyhow::Result<TransactionReceipt> {
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
    let tx_id = transaction_repo::insert_dividend(db, asset.id, &order).await?;

    // Invalidate snapshots from the dividend date
    portfolio_history_repo::delete_from_date(db, &order_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &order_date, asset.id).await?;

    tracing::info!(
        %ticker,
        amount = order.amount,
        date = %order.date,
        "dividend recorded"
    );
    Ok(TransactionReceipt {
        transaction_id: tx_id,
        summary,
    })
}

pub async fn split(
    db: &DatabaseConnection,
    ticker: String,
    order: SplitOrder,
) -> anyhow::Result<TransactionReceipt> {
    let asset = asset_repo::find_by_ticker(db, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

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

    let tx_id = transaction_repo::insert_split(db, asset.id, &order).await?;

    // Price providers retroactively adjust all historical prices after a split,
    // so the entire price cache for this asset is stale.
    daily_price_repo::delete_all_for_asset(db, asset.id).await?;

    // Invalidate portfolio snapshots from the asset's first transaction,
    // since adjusted prices affect the entire history for this asset.
    portfolio_history_repo::delete_from_date(db, &earliest_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &earliest_date, asset.id).await?;

    tracing::info!(
        %ticker,
        ratio = order.ratio,
        date = %order.date,
        "split recorded"
    );
    Ok(TransactionReceipt {
        transaction_id: tx_id,
        summary,
    })
}

pub async fn delete(db: &DatabaseConnection, id: i32) -> anyhow::Result<TransactionReceipt> {
    let tx = transaction_repo::find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;

    transaction_repo::delete_by_id(db, id).await?;

    portfolio_history_repo::delete_from_date(db, &tx.date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &tx.date, tx.asset_id).await?;

    if tx.is_split() {
        daily_price_repo::delete_all_for_asset(db, tx.asset_id).await?;
    }

    tracing::info!(id, "transaction deleted");
    Ok(TransactionReceipt {
        transaction_id: id,
        summary: format!("Transaction {id} deleted."),
    })
}

pub async fn edit(
    db: &DatabaseConnection,
    id: i32,
    new_date: Option<String>,
    new_quantity: Option<f64>,
    new_price: Option<f64>,
    new_fees: Option<f64>,
) -> anyhow::Result<TransactionReceipt> {
    let tx = transaction_repo::find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;

    let new_price_cents = new_price.map(f64_to_cents);
    let new_fees_cents = new_fees.map(f64_to_cents);

    transaction_repo::update_by_id(
        db,
        id,
        new_date.clone(),
        new_quantity,
        new_price_cents,
        new_fees_cents,
    )
    .await?;

    let invalidation_date = match &new_date {
        Some(d) if d < &tx.date => d.clone(),
        _ => tx.date.clone(),
    };

    portfolio_history_repo::delete_from_date(db, &invalidation_date).await?;
    portfolio_asset_history_repo::delete_from_date_for_asset(db, &invalidation_date, tx.asset_id)
        .await?;

    if tx.is_split() {
        daily_price_repo::delete_all_for_asset(db, tx.asset_id).await?;
    }

    tracing::info!(id, "transaction edited");
    Ok(TransactionReceipt {
        transaction_id: id,
        summary: format!("Transaction {id} updated."),
    })
}
