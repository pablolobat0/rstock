use std::collections::HashMap;

use sea_orm::DatabaseConnection;

use crate::constants::{display_date, MONETARY_MULTIPLIER};
use crate::db::repos::{asset_repo, transaction_repo};
use crate::models::{cents_to_f64, Asset};

pub async fn export_transactions_csv(db: &DatabaseConnection, path: &str) -> anyhow::Result<usize> {
    let transactions = transaction_repo::find_all_ordered_by_date(db, None, None).await?;
    let assets = asset_repo::find_all(db).await?;
    let asset_map: HashMap<i32, &Asset> = assets.iter().map(|a| (a.id, a)).collect();

    let mut wtr = csv::Writer::from_path(path)?;
    wtr.write_record([
        "Date",
        "Ticker",
        "Name",
        "AssetType",
        "Currency",
        "MorningstarCode",
        "AssetClass",
        "EquityStyle",
        "BondCredit",
        "BondDuration",
        "Management",
        "Type",
        "Quantity",
        "Price",
        "Fees",
    ])?;

    let decimals = (MONETARY_MULTIPLIER as u64).trailing_zeros() as usize;
    for tx in &transactions {
        let asset = asset_map.get(&tx.asset_id);
        let ticker = asset.map_or("unknown", |a| a.ticker.as_str());
        let name = asset.map_or("", |a| a.name.as_str());
        let asset_type = asset.map(|a| a.asset_type.to_string()).unwrap_or_default();
        let currency = asset.map_or("", |a| a.currency.as_str());
        let morningstar_code = asset
            .and_then(|a| a.morningstar_code.as_deref())
            .unwrap_or("");
        let asset_class = asset.and_then(|a| a.asset_class.as_deref()).unwrap_or("");
        let equity_style = asset.and_then(|a| a.equity_style.as_deref()).unwrap_or("");
        let bond_credit = asset.and_then(|a| a.bond_credit.as_deref()).unwrap_or("");
        let bond_duration = asset.and_then(|a| a.bond_duration.as_deref()).unwrap_or("");
        let management = asset.and_then(|a| a.management.as_deref()).unwrap_or("");
        let tx_type = tx.tx_type.to_string();
        wtr.write_record([
            &display_date(&tx.date),
            ticker,
            name,
            asset_type.as_str(),
            currency,
            morningstar_code,
            asset_class,
            equity_style,
            bond_credit,
            bond_duration,
            management,
            tx_type.as_str(),
            &format!("{}", tx.display_quantity()),
            &format!("{:.decimals$}", cents_to_f64(tx.display_price_cents())),
            &format!("{:.decimals$}", cents_to_f64(tx.display_fees_cents())),
        ])?;
    }

    wtr.flush()?;
    Ok(transactions.len())
}
