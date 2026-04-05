use sea_orm::DatabaseConnection;

use crate::services;

pub async fn run(db: &DatabaseConnection, input: String) -> anyhow::Result<()> {
    let count = services::import::import_transactions_csv(db, &input).await?;
    println!("Imported {count} transactions from {input}");
    Ok(())
}
