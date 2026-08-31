use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};

use crate::constants::{display_date, FLOAT_EPSILON};
use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
    transaction_repo,
};
use crate::models::{
    f64_to_cents, BuyOrder, DividendOrder, SellOrder, SplitOrder, Transaction, TransactionListItem,
};
use crate::services::ledger;

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
    let mutation = db.begin().await?;
    let asset = asset_repo::find_by_ticker(&mutation, &ticker).await?.ok_or_else(|| {
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
    let tx_id = transaction_repo::insert_buy(&mutation, asset.id, &order).await?;
    invalidate_snapshots(&mutation, &order_date).await?;
    mutation.commit().await?;

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
    let mutation = db.begin().await?;
    let asset = asset_repo::find_by_ticker(&mutation, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    // Validate holdings at the sell date (accounts for splits)
    let transactions = transaction_repo::find_by_asset_id(&mutation, asset.id).await?;
    let filtered_transactions: Vec<_> = transactions
        .into_iter()
        .filter(|t| t.date <= order.date)
        .collect();
    let net_qty = replay_quantity(asset.id, &filtered_transactions)?;

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
    let tx_id = transaction_repo::insert_sell(&mutation, asset.id, &order).await?;
    invalidate_snapshots(&mutation, &order_date).await?;
    mutation.commit().await?;

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
    let mutation = db.begin().await?;
    let asset = asset_repo::find_by_ticker(&mutation, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    // Validate holdings at dividend date (accounts for splits)
    let transactions = transaction_repo::find_by_asset_id(&mutation, asset.id).await?;
    let filtered_transactions: Vec<_> = transactions
        .into_iter()
        .filter(|t| t.date <= order.date)
        .collect();
    let net_qty = replay_quantity(asset.id, &filtered_transactions)?;

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
    let tx_id = transaction_repo::insert_dividend(&mutation, asset.id, &order).await?;
    invalidate_snapshots(&mutation, &order_date).await?;
    mutation.commit().await?;

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
    let mutation = db.begin().await?;
    let asset = asset_repo::find_by_ticker(&mutation, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    // Validate holdings at split date (accounts for prior splits)
    let transactions = transaction_repo::find_by_asset_id(&mutation, asset.id).await?;
    let earliest_date = transactions
        .first()
        .map_or_else(|| order.date.clone(), |t| t.date.clone());
    let filtered_transactions: Vec<_> = transactions
        .into_iter()
        .filter(|t| t.date <= order.date)
        .collect();
    let net_qty = replay_quantity(asset.id, &filtered_transactions)?;

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

    let tx_id = transaction_repo::insert_split(&mutation, asset.id, &order).await?;

    // Price providers retroactively adjust all historical prices after a split,
    // so the entire price cache for this asset is stale.
    daily_price_repo::delete_all_for_asset(&mutation, asset.id).await?;

    // Invalidate portfolio snapshots from the asset's first transaction,
    // since adjusted prices affect the entire history for this asset.
    invalidate_snapshots(&mutation, &earliest_date).await?;
    mutation.commit().await?;

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
    let mutation = db.begin().await?;
    let tx = transaction_repo::find_by_id(&mutation, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;
    let invalidation_date = if tx.is_split() {
        earliest_transaction_date(
            &transaction_repo::find_by_asset_id(&mutation, tx.asset_id).await?,
            None,
        )
    } else {
        tx.date.clone()
    };

    transaction_repo::delete_by_id(&mutation, id).await?;
    invalidate_snapshots(&mutation, &invalidation_date).await?;

    if tx.is_split() {
        daily_price_repo::delete_all_for_asset(&mutation, tx.asset_id).await?;
    }
    mutation.commit().await?;

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
    let mutation = db.begin().await?;
    let tx = transaction_repo::find_by_id(&mutation, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;

    let new_price_cents = new_price.map(f64_to_cents);
    let new_fees_cents = new_fees.map(f64_to_cents);

    transaction_repo::update_by_id(
        &mutation,
        id,
        new_date.clone(),
        new_quantity,
        new_price_cents,
        new_fees_cents,
    )
    .await?;

    let invalidation_date = if tx.is_split() {
        let transactions = transaction_repo::find_by_asset_id(&mutation, tx.asset_id).await?;
        earliest_transaction_date(&transactions, new_date.as_deref())
    } else {
        match &new_date {
            Some(d) if d < &tx.date => d.clone(),
            _ => tx.date.clone(),
        }
    };

    invalidate_snapshots(&mutation, &invalidation_date).await?;

    if tx.is_split() {
        daily_price_repo::delete_all_for_asset(&mutation, tx.asset_id).await?;
    }
    mutation.commit().await?;

    tracing::info!(id, "transaction edited");
    Ok(TransactionReceipt {
        transaction_id: id,
        summary: format!("Transaction {id} updated."),
    })
}

async fn invalidate_snapshots(db: &impl ConnectionTrait, date: &str) -> anyhow::Result<()> {
    portfolio_history_repo::delete_from_date(db, date).await?;
    portfolio_asset_history_repo::delete_from_date(db, date).await?;
    Ok(())
}

fn earliest_transaction_date(
    transactions: &[Transaction],
    additional_date: Option<&str>,
) -> String {
    transactions
        .iter()
        .map(|transaction| transaction.date.as_str())
        .chain(additional_date)
        .min()
        .unwrap_or_default()
        .to_owned()
}

fn replay_quantity(asset_id: i32, transactions: &[Transaction]) -> anyhow::Result<f64> {
    let canonical = ledger::CanonicalLedger::from_transactions(asset_id, transactions)
        .map_err(|error| anyhow::anyhow!(error))?;
    Ok(canonical
        .replay()
        .map_err(|error| anyhow::anyhow!(error))?
        .final_quantity)
}
