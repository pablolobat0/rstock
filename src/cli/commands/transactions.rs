use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::cli::display::format_transaction_detail;
use crate::constants::format_date;
use crate::db::repos::{asset_repo, transaction_repo};
use crate::models::{AssetInfo, AssetType, BuyOrder, DividendOrder, SellOrder, SplitOrder};
use crate::services;
use crate::utils::confirm_action;

#[allow(clippy::too_many_arguments)]
pub async fn buy(
    db: &DatabaseConnection,
    ticker: String,
    name: String,
    asset_type: AssetType,
    date: NaiveDate,
    quantity: f64,
    price: f64,
    fees: f64,
    currency: String,
) -> anyhow::Result<()> {
    let asset = AssetInfo {
        ticker,
        name,
        asset_type,
        currency,
    };
    let order = BuyOrder {
        date: format_date(date),
        quantity,
        price,
        fees,
    };
    services::transactions::buy(db, asset, order).await
}

pub async fn sell(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    quantity: f64,
    price: f64,
    fees: f64,
) -> anyhow::Result<()> {
    let order = SellOrder {
        date: format_date(date),
        quantity,
        price,
        fees,
    };
    services::transactions::sell(db, ticker, order).await
}

pub async fn dividend(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    amount: f64,
    fees: f64,
) -> anyhow::Result<()> {
    let order = DividendOrder {
        date: format_date(date),
        amount,
        fees,
    };
    services::transactions::dividend(db, ticker, order).await
}

pub async fn split(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    ratio: f64,
) -> anyhow::Result<()> {
    let order = SplitOrder {
        date: format_date(date),
        ratio,
    };
    services::transactions::split(db, ticker, order).await
}

pub async fn edit(
    db: &DatabaseConnection,
    id: i32,
    date: Option<NaiveDate>,
    quantity: Option<f64>,
    price: Option<f64>,
    fees: Option<f64>,
    yes: bool,
) -> anyhow::Result<()> {
    if date.is_none() && quantity.is_none() && price.is_none() && fees.is_none() {
        anyhow::bail!("At least one field must be specified (--date, --quantity, --price, --fees)");
    }

    let tx = transaction_repo::find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;

    let assets = asset_repo::find_by_ids(db, [tx.asset_id]).await?;
    let ticker = assets.first().map_or("unknown", |a| a.ticker.as_str());

    println!("Current transaction:");
    println!("{}", format_transaction_detail(&tx, ticker));

    if !yes && !confirm_action("Apply changes?") {
        println!("Cancelled.");
        return Ok(());
    }

    let new_date = date.map(format_date);
    services::transactions::edit(db, id, new_date, quantity, price, fees).await
}

pub async fn delete(db: &DatabaseConnection, id: i32, yes: bool) -> anyhow::Result<()> {
    let tx = transaction_repo::find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;

    let assets = asset_repo::find_by_ids(db, [tx.asset_id]).await?;
    let ticker = assets.first().map_or("unknown", |a| a.ticker.as_str());

    println!("Transaction to delete:");
    println!("{}", format_transaction_detail(&tx, ticker));

    if !yes && !confirm_action("Delete this transaction?") {
        println!("Cancelled.");
        return Ok(());
    }

    services::transactions::delete(db, id).await
}
