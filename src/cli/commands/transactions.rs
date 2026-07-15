use chrono::NaiveDate;
use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::cli::display::{
    format_transaction_detail, print_transaction_list, transaction_list_output,
};
use crate::cli::output::{self, OutputFormat};
use crate::constants::format_date;
use crate::db::repos::{asset_repo, transaction_repo};
use crate::models::{BuyOrder, DividendOrder, SellOrder, SplitOrder};
use crate::services;
use crate::utils::confirm_action;

pub async fn list(db: &DatabaseConnection, output_format: OutputFormat) -> anyhow::Result<()> {
    let items = services::transactions::list(db).await?;
    if output_format.is_json() {
        output::emit_json("transaction.list", &transaction_list_output(&items))?;
    } else {
        print_transaction_list(&items);
    }
    Ok(())
}

pub async fn buy(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    quantity: f64,
    price: f64,
    fees: f64,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let order = BuyOrder {
        date: format_date(date),
        quantity,
        price,
        fees,
    };
    let receipt = services::transactions::buy(db, ticker, order).await?;
    emit_created_receipt("transaction.buy", &receipt, output_format)
}

pub async fn sell(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    quantity: f64,
    price: f64,
    fees: f64,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let order = SellOrder {
        date: format_date(date),
        quantity,
        price,
        fees,
    };
    let receipt = services::transactions::sell(db, ticker, order).await?;
    emit_created_receipt("transaction.sell", &receipt, output_format)
}

pub async fn dividend(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    amount: f64,
    fees: f64,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let order = DividendOrder {
        date: format_date(date),
        amount,
        fees,
    };
    let receipt = services::transactions::dividend(db, ticker, order).await?;
    emit_created_receipt("transaction.dividend", &receipt, output_format)
}

pub async fn split(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    ratio: f64,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let order = SplitOrder {
        date: format_date(date),
        ratio,
    };
    let receipt = services::transactions::split(db, ticker, order).await?;
    emit_created_receipt("transaction.split", &receipt, output_format)
}

pub struct EditOptions {
    pub date: Option<NaiveDate>,
    pub quantity: Option<f64>,
    pub price: Option<f64>,
    pub fees: Option<f64>,
    pub yes: bool,
}

pub async fn edit(
    db: &DatabaseConnection,
    id: i32,
    options: EditOptions,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    if options.date.is_none()
        && options.quantity.is_none()
        && options.price.is_none()
        && options.fees.is_none()
    {
        anyhow::bail!("At least one field must be specified (--date, --quantity, --price, --fees)");
    }

    require_json_consent(output_format, options.yes)?;

    let tx = transaction_repo::find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;

    let assets = asset_repo::find_by_ids(db, [tx.asset_id]).await?;
    let ticker = assets.first().map_or("unknown", |a| a.ticker.as_str());

    if !output_format.is_json() {
        println!("Current transaction:");
        println!("{}", format_transaction_detail(&tx, ticker));

        if !options.yes && !confirm_action("Apply changes?") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let new_date = options.date.map(format_date);
    let receipt = services::transactions::edit(
        db,
        id,
        new_date,
        options.quantity,
        options.price,
        options.fees,
    )
    .await?;
    emit_affected_receipt("transaction.edit", &receipt, output_format)
}

pub async fn delete(
    db: &DatabaseConnection,
    id: i32,
    yes: bool,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    require_json_consent(output_format, yes)?;

    let tx = transaction_repo::find_by_id(db, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Transaction {id} not found"))?;

    let assets = asset_repo::find_by_ids(db, [tx.asset_id]).await?;
    let ticker = assets.first().map_or("unknown", |a| a.ticker.as_str());

    if !output_format.is_json() {
        println!("Transaction to delete:");
        println!("{}", format_transaction_detail(&tx, ticker));

        if !yes && !confirm_action("Delete this transaction?") {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let receipt = services::transactions::delete(db, id).await?;
    emit_affected_receipt("transaction.delete", &receipt, output_format)
}

#[derive(Serialize)]
struct TransactionIdOutput {
    transaction_id: i32,
}

fn emit_created_receipt(
    command: &str,
    receipt: &services::transactions::TransactionReceipt,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    if output_format.is_json() {
        output::emit_json(
            command,
            &TransactionIdOutput {
                transaction_id: receipt.transaction_id,
            },
        )
    } else {
        println!("{}", receipt.summary);
        println!("Transaction ID: {}", receipt.transaction_id);
        Ok(())
    }
}

fn emit_affected_receipt(
    command: &str,
    receipt: &services::transactions::TransactionReceipt,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    if output_format.is_json() {
        output::emit_json(
            command,
            &TransactionIdOutput {
                transaction_id: receipt.transaction_id,
            },
        )
    } else {
        println!("{}", receipt.summary);
        Ok(())
    }
}

fn require_json_consent(output_format: OutputFormat, yes: bool) -> anyhow::Result<()> {
    if output_format.is_json() && !yes {
        anyhow::bail!("--yes is required for transaction edit/delete in JSON mode");
    }
    Ok(())
}
