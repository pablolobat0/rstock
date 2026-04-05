use anyhow::{bail, Context};
use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, DISPLAY_DATE_FORMAT};
use crate::models::{
    AssetInfo, AssetType, BuyOrder, CsvRow, DividendOrder, SellOrder, SplitOrder, TxType,
};
use crate::services::transactions;

pub async fn import_transactions_csv(db: &DatabaseConnection, path: &str) -> anyhow::Result<usize> {
    let mut rdr =
        csv::Reader::from_path(path).with_context(|| format!("failed to open CSV file: {path}"))?;

    let mut rows = Vec::new();
    for (i, result) in rdr.records().enumerate() {
        let record = result.with_context(|| format!("row {}: failed to read CSV record", i + 2))?;
        let row = parse_row(&record, i + 2)?;
        rows.push(row);
    }

    rows.sort_by_key(|r| r.date);

    let count = rows.len();
    for row in &rows {
        let row_num = row.source_row;
        let date_str = format_date(row.date);

        match row.tx_type {
            TxType::Buy => {
                let name = row
                    .name
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!("row {row_num}: buy transaction requires a non-empty Name")
                    })?;
                let asset_type = row.asset_type.clone().ok_or_else(|| {
                    anyhow::anyhow!("row {row_num}: buy transaction requires a non-empty AssetType")
                })?;
                let currency = row
                    .currency
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "row {row_num}: buy transaction requires a non-empty Currency"
                        )
                    })?;

                let asset_info = AssetInfo {
                    ticker: row.ticker.clone(),
                    name: name.to_string(),
                    asset_type,
                    currency: currency.to_string(),
                };
                let order = BuyOrder {
                    date: date_str,
                    quantity: row.quantity,
                    price: row.price,
                    fees: row.fees,
                };
                transactions::buy(db, asset_info, order)
                    .await
                    .with_context(|| format!("row {row_num}"))?;
            }
            TxType::Sell => {
                let order = SellOrder {
                    date: date_str,
                    quantity: row.quantity,
                    price: row.price,
                    fees: row.fees,
                };
                transactions::sell(db, row.ticker.clone(), order)
                    .await
                    .with_context(|| format!("row {row_num}"))?;
            }
            TxType::Dividend => {
                let order = DividendOrder {
                    date: date_str,
                    amount: row.price,
                    fees: row.fees,
                };
                transactions::dividend(db, row.ticker.clone(), order)
                    .await
                    .with_context(|| format!("row {row_num}"))?;
            }
            TxType::Split => {
                let order = SplitOrder {
                    date: date_str,
                    ratio: row.quantity,
                };
                transactions::split(db, row.ticker.clone(), order)
                    .await
                    .with_context(|| format!("row {row_num}"))?;
            }
        }
    }

    Ok(count)
}

fn parse_row(record: &csv::StringRecord, row_num: usize) -> anyhow::Result<CsvRow> {
    if record.len() != 9 {
        bail!("row {row_num}: expected 9 fields, got {}", record.len());
    }

    let date = NaiveDate::parse_from_str(record[0].trim(), DISPLAY_DATE_FORMAT)
        .with_context(|| format!("row {row_num}: invalid date '{}'", &record[0]))?;

    let today = chrono::Local::now().date_naive();
    if date > today {
        bail!(
            "row {row_num}: date cannot be in the future: {}",
            &record[0]
        );
    }

    let ticker = record[1].trim().to_string();
    if ticker.is_empty() {
        bail!("row {row_num}: ticker cannot be empty");
    }

    let name = parse_optional(&record[2]);
    let asset_type = parse_optional(&record[3])
        .map(|s| s.parse::<AssetType>())
        .transpose()
        .with_context(|| format!("row {row_num}: invalid asset type '{}'", &record[3]))?;
    let currency = parse_optional(&record[4]);

    let tx_type = record[5]
        .trim()
        .parse::<TxType>()
        .with_context(|| format!("row {row_num}: invalid transaction type '{}'", &record[5]))?;

    let quantity = record[6]
        .trim()
        .parse::<f64>()
        .with_context(|| format!("row {row_num}: invalid quantity '{}'", &record[6]))?;

    let price = record[7]
        .trim()
        .parse::<f64>()
        .with_context(|| format!("row {row_num}: invalid price '{}'", &record[7]))?;

    let fees = record[8]
        .trim()
        .parse::<f64>()
        .with_context(|| format!("row {row_num}: invalid fees '{}'", &record[8]))?;

    Ok(CsvRow {
        source_row: row_num,
        date,
        ticker,
        name,
        asset_type,
        currency,
        tx_type,
        quantity,
        price,
        fees,
    })
}

fn parse_optional(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
