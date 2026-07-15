use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::cli::output::{self, OutputFormat};
use crate::services;

#[derive(Serialize)]
struct ImportOutput<'a> {
    count: usize,
    path: &'a str,
}

pub async fn run(
    db: &DatabaseConnection,
    input: String,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let result = services::import::import_transactions_csv(db, &input).await?;
    if output_format.is_json() {
        output::emit_json(
            "transaction.import",
            &ImportOutput {
                count: result.count,
                path: &input,
            },
        )?;
    } else {
        for receipt in result.transaction_receipts {
            println!("{}", receipt.summary);
            println!("Transaction ID: {}", receipt.transaction_id);
        }
        println!("Imported {} transactions from {input}", result.count);
    }
    Ok(())
}
