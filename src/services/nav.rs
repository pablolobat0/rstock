use std::collections::HashMap;

use anyhow::Context;
use chrono::NaiveDate;
use sea_orm::*;

use crate::db::entities::{asset, portfolio_history, transaction};
use crate::services::daily_prices;

pub async fn rebuild_portfolio_history(
    db: &DatabaseConnection,
    rebuild_from_date: Option<String>,
) -> anyhow::Result<()> {
    // Load all transactions ordered by date ASC
    let transactions = transaction::Entity::find()
        .order_by_asc(transaction::Column::Date)
        .all(db)
        .await?;

    if transactions.is_empty() {
        return Ok(());
    }

    let first_tx_date = &transactions[0].date;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Determine start_date
    let start_date_str = match &rebuild_from_date {
        Some(d) => d.clone(),
        None => {
            // Check if we have existing history — only rebuild from last date
            if let Some(latest) = get_latest_snapshot(db).await? {
                // Rebuild from latest date (to fill in new days)
                latest.date
            } else {
                first_tx_date.clone()
            }
        }
    };

    let start_date = NaiveDate::parse_from_str(&start_date_str, "%Y-%m-%d")
        .context("invalid start date")?;
    let today_date = NaiveDate::parse_from_str(&today, "%Y-%m-%d")
        .context("invalid today date")?;

    // Delete portfolio_history records from start_date onwards
    portfolio_history::Entity::delete_many()
        .filter(portfolio_history::Column::Date.gte(&start_date_str))
        .exec(db)
        .await?;

    // Load snapshot for day before start_date to get previous state
    let prev_date = (start_date - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let prev_snapshot = get_snapshot_at_or_before(db, &prev_date).await?;

    let cash_balance: f64 = prev_snapshot
        .as_ref()
        .map(|s| s.cash_balance)
        .unwrap_or(0.0);
    let mut outstanding_shares: f64 = prev_snapshot
        .as_ref()
        .map(|s| s.outstanding_shares)
        .unwrap_or(0.0);
    let mut nav: f64 = prev_snapshot.as_ref().map(|s| s.nav).unwrap_or(100.0);

    // Reconstruct holdings from all transactions before start_date
    let mut holdings: HashMap<i32, f64> = HashMap::new();
    for tx in &transactions {
        if tx.date < start_date_str {
            *holdings.entry(tx.asset_id).or_insert(0.0) += tx.quantity;
        }
    }

    // Build a map of transactions by date for quick lookup
    let mut tx_by_date: HashMap<String, Vec<&transaction::Model>> = HashMap::new();
    for tx in &transactions {
        if tx.date >= start_date_str && tx.date <= today {
            tx_by_date
                .entry(tx.date.clone())
                .or_default()
                .push(tx);
        }
    }

    // Collect all asset IDs that we need prices for
    let mut needed_asset_ids: std::collections::HashSet<i32> = holdings.keys().copied().collect();
    for tx in &transactions {
        if tx.date >= start_date_str && tx.date <= today {
            needed_asset_ids.insert(tx.asset_id);
        }
    }

    // Load asset models
    let assets: Vec<asset::Model> = asset::Entity::find()
        .filter(asset::Column::Id.is_in(needed_asset_ids.iter().copied()))
        .all(db)
        .await?;
    let asset_map: HashMap<i32, &asset::Model> =
        assets.iter().map(|a| (a.id, a)).collect();

    // Fill price caches for all needed assets
    for asset in &assets {
        if let Err(e) =
            daily_prices::fill_prices_for_range(db, asset, &start_date_str, &today).await
        {
            eprintln!(
                "Warning: failed to fill prices for {}: {}",
                asset.ticker, e
            );
        }
    }

    // Iterate each calendar day from start_date to today
    let mut current = start_date;
    while current <= today_date {
        let date_str = current.format("%Y-%m-%d").to_string();

        // Process buy transactions on this day
        if let Some(day_txs) = tx_by_date.get(&date_str) {
            for tx in day_txs {
                let deposit = tx.quantity * (tx.price_cents as f64 / 100.0)
                    + (tx.fees_cents as f64 / 100.0);

                if outstanding_shares == 0.0 {
                    nav = 100.0;
                    let shares_issued = deposit / 100.0;
                    outstanding_shares = shares_issued;
                } else {
                    let shares_issued = deposit / nav;
                    outstanding_shares += shares_issued;
                }

                *holdings.entry(tx.asset_id).or_insert(0.0) += tx.quantity;
                // cash_balance stays 0 (deposit + immediate spend)
            }
        }

        // Skip if no portfolio yet
        if outstanding_shares == 0.0 {
            current += chrono::Duration::days(1);
            continue;
        }

        // Compute EOD values
        let mut asset_value = 0.0;
        for (&asset_id, &qty) in &holdings {
            if qty <= 0.0 {
                continue;
            }
            if let Some(asset_model) = asset_map.get(&asset_id) {
                if let Some(closing_price) =
                    daily_prices::get_closing_price(db, asset_model, &date_str).await?
                {
                    asset_value += qty * closing_price;
                }
            }
        }

        let total_value = cash_balance + asset_value;
        if outstanding_shares > 0.0 {
            nav = total_value / outstanding_shares;
        }

        // Upsert portfolio_history record
        let existing = portfolio_history::Entity::find_by_id(&date_str)
            .one(db)
            .await?;

        if let Some(record) = existing {
            let mut active: portfolio_history::ActiveModel = record.into();
            active.cash_balance = Set(cash_balance);
            active.asset_value = Set(asset_value);
            active.total_value = Set(total_value);
            active.outstanding_shares = Set(outstanding_shares);
            active.nav = Set(nav);
            active.update(db).await?;
        } else {
            let record = portfolio_history::ActiveModel {
                date: Set(date_str),
                cash_balance: Set(cash_balance),
                asset_value: Set(asset_value),
                total_value: Set(total_value),
                outstanding_shares: Set(outstanding_shares),
                nav: Set(nav),
            };
            record.insert(db).await?;
        }

        current += chrono::Duration::days(1);
    }

    Ok(())
}

pub async fn get_latest_snapshot(
    db: &DatabaseConnection,
) -> anyhow::Result<Option<portfolio_history::Model>> {
    let snapshot = portfolio_history::Entity::find()
        .order_by_desc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(snapshot)
}

pub async fn get_snapshot_at_or_before(
    db: &DatabaseConnection,
    date: &str,
) -> anyhow::Result<Option<portfolio_history::Model>> {
    let snapshot = portfolio_history::Entity::find()
        .filter(portfolio_history::Column::Date.lte(date))
        .order_by_desc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(snapshot)
}

pub async fn get_earliest_snapshot(
    db: &DatabaseConnection,
) -> anyhow::Result<Option<portfolio_history::Model>> {
    let snapshot = portfolio_history::Entity::find()
        .order_by_asc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(snapshot)
}
