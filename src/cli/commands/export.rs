use sea_orm::DatabaseConnection;
use serde::Serialize;

use crate::cli::output::{self, OutputFormat};
use crate::services;

#[derive(Serialize)]
struct ExportOutput<'a> {
    count: usize,
    path: &'a str,
}

pub async fn run(
    db: &DatabaseConnection,
    output_path: String,
    output_format: OutputFormat,
) -> anyhow::Result<()> {
    let count = services::export::export_transactions_csv(db, &output_path).await?;
    if output_format.is_json() {
        output::emit_json(
            "transaction.export",
            &ExportOutput {
                count,
                path: &output_path,
            },
        )?;
    } else {
        println!("Exported {count} transactions to {output_path}");
    }
    Ok(())
}
