use crate::constants::display_date;
use crate::models::{cents_to_f64, Transaction, TransactionListItem};
use serde::Serialize;

use super::types::TransactionRow;

pub fn print_transaction_list(items: &[TransactionListItem]) {
    if items.is_empty() {
        println!("No transactions found.");
        return;
    }

    let rows = transaction_rows(items);

    println!("{}", tabled::Table::new(&rows));
    println!("\nTotal: {} transactions", items.len());
}

#[derive(Serialize)]
pub struct TransactionListOutput {
    pub transactions: Vec<TransactionRow>,
    pub count: usize,
}

pub fn transaction_list_output(items: &[TransactionListItem]) -> TransactionListOutput {
    TransactionListOutput {
        transactions: transaction_rows(items),
        count: items.len(),
    }
}

fn transaction_rows(items: &[TransactionListItem]) -> Vec<TransactionRow> {
    items
        .iter()
        .map(|item| TransactionRow {
            id: item.transaction.id,
            date: display_date(&item.transaction.date),
            tx_type: item.transaction.tx_type.to_string(),
            ticker: item.ticker.clone(),
            asset_name: item.asset_name.clone(),
            quantity: item.transaction.quantity,
            price: cents_to_f64(item.transaction.price_cents),
            fees: cents_to_f64(item.transaction.fees_cents),
        })
        .collect()
}

pub fn format_transaction_detail(tx: &Transaction, ticker: &str) -> String {
    format!(
        "  ID:       {}\n  Type:     {}\n  Ticker:   {}\n  Date:     {}\n  Quantity: {:.4}\n  Price:    {:.4}\n  Fees:     {:.4}",
        tx.id,
        tx.tx_type,
        ticker,
        display_date(&tx.date),
        tx.quantity,
        cents_to_f64(tx.price_cents),
        cents_to_f64(tx.fees_cents),
    )
}
