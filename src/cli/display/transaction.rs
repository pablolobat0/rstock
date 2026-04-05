use crate::constants::display_date;
use crate::models::{cents_to_f64, Transaction};

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
