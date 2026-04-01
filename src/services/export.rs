use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::constants::{display_date, MONETARY_MULTIPLIER};
use crate::db::repos::{asset_repo, transaction_repo};
use crate::models::cents_to_f64;

pub async fn export_transactions_csv(db: &DatabaseConnection, path: &str) -> anyhow::Result<usize> {
    let transactions = transaction_repo::find_all_ordered_by_date(db).await?;
    let assets = asset_repo::find_all(db).await?;
    let asset_map: HashMap<i32, &str> = assets.iter().map(|a| (a.id, a.ticker.as_str())).collect();

    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record(["Date", "Ticker", "Type", "Quantity", "Price", "Fees"])?;

    let decimals = (MONETARY_MULTIPLIER as u64).trailing_zeros() as usize;
    for tx in &transactions {
        let ticker = asset_map.get(&tx.asset_id).copied().unwrap_or("unknown");
        let tx_type = tx.tx_type.to_string();
        wtr.write_record([
            &display_date(&tx.date),
            ticker,
            tx_type.as_str(),
            &format!("{}", tx.quantity),
            &format!("{:.decimals$}", cents_to_f64(tx.price_cents)),
            &format!("{:.decimals$}", cents_to_f64(tx.fees_cents)),
        ])?;
    }

    wtr.flush()?;
    Ok(transactions.len())
}
