use sea_orm::DatabaseConnection;

use crate::services;

pub async fn run(db: &DatabaseConnection, output: String) -> anyhow::Result<()> {
    let count = services::export::export_transactions_csv(db, &output).await?;
    println!("Exported {count} transactions to {output}");
    Ok(())
}
