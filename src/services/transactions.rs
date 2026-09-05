use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};

use crate::constants::{display_date, DATE_FORMAT, MONETARY_MULTIPLIER};
use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
    transaction_repo,
};
use crate::models::{
    f64_to_cents, BuyOrder, DividendOrder, SellOrder, SplitOrder, Transaction, TransactionListItem,
};
use crate::services::ledger::{
    CanonicalLedger, LedgerEffect, LedgerEntry, LedgerEntryKind, LedgerReplay, LedgerTransition,
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
    validate_buy_order(&order)?;
    let mutation = db.begin().await?;
    let asset = asset_repo::find_by_ticker(&mutation, &ticker).await?.ok_or_else(|| {
        anyhow::anyhow!(
            "asset with ticker '{ticker}' not found; create it first with `rstock portfolio asset add -t {ticker} ...`"
        )
    })?;

    let tx_id = transaction_repo::insert_buy(&mutation, asset.id, &order).await?;
    let replay = replay_asset_ledger(&mutation, asset.id).await?;
    let total = match effect_for_entry(&replay, tx_id)? {
        LedgerEffect::Buy { contribution } => contribution / MONETARY_MULTIPLIER,
        _ => anyhow::bail!("inserted buy transaction {tx_id} has an unexpected replay effect"),
    };
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
    validate_sell_order(&order)?;
    let mutation = db.begin().await?;
    let asset = asset_repo::find_by_ticker(&mutation, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    let tx_id = transaction_repo::insert_sell(&mutation, asset.id, &order).await?;
    let replay = replay_asset_ledger(&mutation, asset.id).await?;
    let proceeds = match effect_for_entry(&replay, tx_id)? {
        LedgerEffect::Sell { withdrawal, .. } => withdrawal / MONETARY_MULTIPLIER,
        _ => anyhow::bail!("inserted sell transaction {tx_id} has an unexpected replay effect"),
    };
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
    validate_dividend_order(&order)?;
    let mutation = db.begin().await?;
    let asset = asset_repo::find_by_ticker(&mutation, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    let tx_id = transaction_repo::insert_dividend(&mutation, asset.id, &order).await?;
    let replay = replay_asset_ledger(&mutation, asset.id).await?;
    let net_amount = match effect_for_entry(&replay, tx_id)? {
        LedgerEffect::Dividend { net_income } => net_income / MONETARY_MULTIPLIER,
        _ => anyhow::bail!("inserted dividend transaction {tx_id} has an unexpected replay effect"),
    };
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
    validate_split_order(&order)?;
    let mutation = db.begin().await?;
    let asset = asset_repo::find_by_ticker(&mutation, &ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Asset with ticker '{ticker}' not found"))?;

    let tx_id = transaction_repo::insert_split(&mutation, asset.id, &order).await?;
    let replay = replay_asset_ledger(&mutation, asset.id).await?;
    let transition = transition_for_entry(&replay, tx_id)?;
    let summary = format!(
        "Split {} ({}): ratio {}, holdings {:.4} -> {:.4} on {}",
        asset.name,
        ticker,
        order.ratio,
        transition.quantity_before,
        transition.quantity_after,
        display_date(&order.date)
    );

    let transactions = transaction_repo::find_by_asset_id(&mutation, asset.id).await?;
    let earliest_date = earliest_transaction_date(&transactions, None);

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

    transaction_repo::delete_by_id(&mutation, id).await?;

    // Deletion is a ledger mutation, not merely a row removal.  Replay the
    // exact resulting asset ledger so that a deleted acquisition or split
    // cannot leave a later entry with an invalid prefix.
    replay_asset_ledger(&mutation, tx.asset_id).await?;

    let remaining_transactions = transaction_repo::find_by_asset_id(&mutation, tx.asset_id).await?;
    let invalidation_date = if tx.is_split() {
        // A split changes the interpretation of the complete historical
        // price cache. Include the old date even when it was the first entry
        // and is no longer present after deletion.
        earliest_transaction_date(&remaining_transactions, Some(&tx.date))
    } else {
        tx.date.clone()
    };
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

    validate_edit(&tx, new_date.as_deref(), new_quantity, new_price, new_fees)?;

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

    // Replaying after tentative persistence validates every later prefix,
    // including entries whose meaning changed because this entry moved in
    // chronological order. Any error rolls back the update and all
    // invalidation work below with the caller-owned transaction.
    replay_asset_ledger(&mutation, tx.asset_id).await?;

    let invalidation_date = if tx.is_split() {
        let transactions = transaction_repo::find_by_asset_id(&mutation, tx.asset_id).await?;
        earliest_transaction_date(&transactions, Some(&tx.date))
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

async fn replay_asset_ledger(
    db: &impl ConnectionTrait,
    asset_id: i32,
) -> anyhow::Result<LedgerReplay> {
    let transactions = transaction_repo::find_by_asset_id(db, asset_id).await?;
    let entries = transactions.iter().map(ledger_entry).collect();
    CanonicalLedger::new(asset_id, entries)
        .and_then(|ledger| ledger.replay())
        .map_err(anyhow::Error::from)
        .context("transaction ledger replay failed")
}

fn effect_for_entry(replay: &LedgerReplay, transaction_id: i32) -> anyhow::Result<&LedgerEffect> {
    Ok(&transition_for_entry(replay, transaction_id)?.effect)
}

fn transition_for_entry(
    replay: &LedgerReplay,
    transaction_id: i32,
) -> anyhow::Result<&LedgerTransition> {
    replay
        .transitions
        .iter()
        .find(|transition| transition.entry.id == transaction_id)
        .ok_or_else(|| anyhow::anyhow!("transaction {transaction_id} was not replayed"))
}

fn validate_edit(
    transaction: &Transaction,
    new_date: Option<&str>,
    new_quantity: Option<f64>,
    new_price: Option<f64>,
    new_fees: Option<f64>,
) -> anyhow::Result<()> {
    if let Some(date) = new_date {
        validate_date(date)?;
    }

    match &transaction.tx_type {
        crate::models::TxType::Buy | crate::models::TxType::Sell => {
            if let Some(quantity) = new_quantity {
                validate_positive(quantity, "quantity")?;
            }
            if let Some(price) = new_price {
                validate_positive(price, "price")?;
            }
            if let Some(fees) = new_fees {
                validate_non_negative(fees, "fees")?;
            }
        }
        crate::models::TxType::Dividend => {
            if new_quantity.is_some() {
                anyhow::bail!("quantity cannot be edited for a dividend; use --price for amount")
            }
            if let Some(price) = new_price {
                validate_positive(price, "amount")?;
            }
            if let Some(fees) = new_fees {
                validate_non_negative(fees, "fees")?;
            }

            let gross_cents = new_price.map_or(transaction.price_cents, f64_to_cents);
            let deductions_cents = new_fees.map_or(transaction.fees_cents, f64_to_cents);
            if deductions_cents > gross_cents {
                anyhow::bail!("fees must not exceed dividend amount");
            }
        }
        crate::models::TxType::Split => {
            if new_quantity.is_none() && new_price.is_none() && new_fees.is_none() {
                // Date-only split corrections are meaningful and valid.
                return Ok(());
            }
            if let Some(quantity) = new_quantity {
                validate_positive(quantity, "ratio")?;
            }
            if new_price.is_some() {
                anyhow::bail!("price cannot be edited for a split; use --quantity for ratio")
            }
            if new_fees.is_some() {
                anyhow::bail!("fees cannot be edited for a split")
            }
        }
    }
    Ok(())
}

fn ledger_entry(transaction: &Transaction) -> LedgerEntry {
    let kind = match &transaction.tx_type {
        crate::models::TxType::Buy => LedgerEntryKind::Buy {
            units: transaction.quantity,
            unit_price_cents: transaction.price_cents,
            fees_cents: transaction.fees_cents,
        },
        crate::models::TxType::Sell => LedgerEntryKind::Sell {
            units: transaction.quantity,
            unit_price_cents: transaction.price_cents,
            fees_cents: transaction.fees_cents,
        },
        crate::models::TxType::Dividend => LedgerEntryKind::Dividend {
            gross_amount_cents: transaction.price_cents,
            deductions_cents: transaction.fees_cents,
        },
        crate::models::TxType::Split => LedgerEntryKind::Split {
            ratio: transaction.quantity,
        },
    };
    LedgerEntry {
        id: transaction.id,
        asset_id: transaction.asset_id,
        date: transaction.date.clone(),
        kind,
    }
}

fn validate_buy_order(order: &BuyOrder) -> anyhow::Result<()> {
    validate_trade_shape(&order.date, order.quantity, order.price, order.fees)
}

fn validate_sell_order(order: &SellOrder) -> anyhow::Result<()> {
    validate_trade_shape(&order.date, order.quantity, order.price, order.fees)
}

fn validate_trade_shape(date: &str, quantity: f64, price: f64, fees: f64) -> anyhow::Result<()> {
    validate_date(date)?;
    validate_positive(quantity, "quantity")?;
    validate_positive(price, "price")?;
    validate_non_negative(fees, "fees")
}

fn validate_dividend_order(order: &DividendOrder) -> anyhow::Result<()> {
    validate_date(&order.date)?;
    validate_positive(order.amount, "amount")?;
    validate_non_negative(order.fees, "fees")?;
    if order.fees > order.amount {
        anyhow::bail!("fees must not exceed dividend amount");
    }
    Ok(())
}

fn validate_split_order(order: &SplitOrder) -> anyhow::Result<()> {
    validate_date(&order.date)?;
    validate_positive(order.ratio, "ratio")
}

fn validate_date(date: &str) -> anyhow::Result<()> {
    let parsed = NaiveDate::parse_from_str(date, DATE_FORMAT)
        .map_err(|_| anyhow::anyhow!("invalid date '{date}', expected YYYY-MM-DD format"))?;
    if parsed.format(DATE_FORMAT).to_string() != date {
        anyhow::bail!("invalid date '{date}', expected YYYY-MM-DD format");
    }
    if parsed > chrono::Local::now().date_naive() {
        anyhow::bail!("date cannot be in the future: {date}");
    }
    Ok(())
}

fn validate_positive(value: f64, field: &str) -> anyhow::Result<()> {
    if !value.is_finite() {
        anyhow::bail!("{field} must be finite");
    }
    if value <= 0.0 {
        anyhow::bail!("{field} must be positive");
    }
    Ok(())
}

fn validate_non_negative(value: f64, field: &str) -> anyhow::Result<()> {
    if !value.is_finite() {
        anyhow::bail!("{field} must be finite");
    }
    if value < 0.0 {
        anyhow::bail!("{field} must be non-negative");
    }
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
