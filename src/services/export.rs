use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::db::repos::{asset_repo, transaction_repo};
use crate::models::cents_to_f64;

pub async fn export_transactions_csv(db: &DatabaseConnection, path: &str) -> anyhow::Result<usize> {
    let transactions = transaction_repo::find_all_ordered_by_date(db).await?;
    let assets = asset_repo::find_all(db).await?;
    let asset_map: HashMap<i32, &str> = assets.iter().map(|a| (a.id, a.ticker.as_str())).collect();

    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record(["Date", "Ticker", "Type", "Quantity", "Price", "Fees"])?;

    for tx in &transactions {
        let ticker = asset_map.get(&tx.asset_id).copied().unwrap_or("unknown");
        wtr.write_record([
            &tx.date,
            ticker,
            &tx.tx_type,
            &format!("{}", tx.quantity),
            &format!("{:.2}", cents_to_f64(tx.price_cents)),
            &format!("{:.2}", cents_to_f64(tx.fees_cents)),
        ])?;
    }

    wtr.flush()?;
    Ok(transactions.len())
}
