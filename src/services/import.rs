use anyhow::{bail, Context};
use chrono::NaiveDate;
use clap::ValueEnum;
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, DISPLAY_DATE_FORMAT};
use crate::db::repos::asset_repo;
use crate::models::{
    AssetClass, AssetClassification, AssetInfo, AssetType, BondCredit, BondDuration, BuyOrder,
    CsvRow, DividendOrder, EquityStyle, Management, SellOrder, SplitOrder, TxType,
};
use crate::services::{assets, transactions};

const EXPECTED_HEADERS: [&str; 15] = [
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
];

pub async fn import_transactions_csv(db: &DatabaseConnection, path: &str) -> anyhow::Result<usize> {
    let mut rdr =
        csv::Reader::from_path(path).with_context(|| format!("failed to open CSV file: {path}"))?;
    validate_headers(rdr.headers()?)?;

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

                if asset_repo::find_by_ticker(db, &row.ticker).await?.is_none() {
                    let asset_info = AssetInfo {
                        ticker: row.ticker.clone(),
                        name: name.to_string(),
                        asset_type,
                        currency: currency.to_string(),
                    };
                    assets::create_tracked_asset(
                        db,
                        &asset_info,
                        &row.classification,
                        row.morningstar_code.as_deref(),
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("row {row_num}: {e}"))?;
                }

                let order = BuyOrder {
                    date: date_str,
                    quantity: row.quantity,
                    price: row.price,
                    fees: row.fees,
                };
                transactions::buy(db, row.ticker.clone(), order)
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
    if record.len() != EXPECTED_HEADERS.len() {
        bail!(
            "row {row_num}: expected {} fields for classified transaction CSV schema, got {}",
            EXPECTED_HEADERS.len(),
            record.len()
        );
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
    let morningstar_code = parse_optional(&record[5]);
    let classification = AssetClassification {
        asset_class: parse_optional_enum::<AssetClass>(&record[6], row_num, "AssetClass")?,
        equity_style: parse_optional_enum::<EquityStyle>(&record[7], row_num, "EquityStyle")?,
        bond_credit: parse_optional_enum::<BondCredit>(&record[8], row_num, "BondCredit")?,
        bond_duration: parse_optional_enum::<BondDuration>(&record[9], row_num, "BondDuration")?,
        management: parse_optional_enum::<Management>(&record[10], row_num, "Management")?,
    };

    let tx_type = record[11]
        .trim()
        .parse::<TxType>()
        .with_context(|| format!("row {row_num}: invalid transaction type '{}'", &record[11]))?;

    let quantity = record[12]
        .trim()
        .parse::<f64>()
        .with_context(|| format!("row {row_num}: invalid quantity '{}'", &record[12]))?;

    let price = record[13]
        .trim()
        .parse::<f64>()
        .with_context(|| format!("row {row_num}: invalid price '{}'", &record[13]))?;

    let fees = record[14]
        .trim()
        .parse::<f64>()
        .with_context(|| format!("row {row_num}: invalid fees '{}'", &record[14]))?;

    validate_numeric_fields(row_num, &tx_type, quantity, price, fees)?;

    Ok(CsvRow {
        source_row: row_num,
        date,
        ticker,
        name,
        asset_type,
        currency,
        morningstar_code,
        classification,
        tx_type,
        quantity,
        price,
        fees,
    })
}

fn validate_numeric_fields(
    row_num: usize,
    tx_type: &TxType,
    quantity: f64,
    price: f64,
    fees: f64,
) -> anyhow::Result<()> {
    if fees < 0.0 {
        bail!("row {row_num}: fees must be non-negative");
    }

    match tx_type {
        TxType::Buy | TxType::Sell => {
            if quantity <= 0.0 {
                bail!("row {row_num}: quantity must be positive");
            }
            if price <= 0.0 {
                bail!("row {row_num}: price must be positive");
            }
        }
        TxType::Dividend => {
            if price <= 0.0 {
                bail!("row {row_num}: dividend amount must be positive");
            }
        }
        TxType::Split => {
            if quantity <= 0.0 {
                bail!("row {row_num}: split ratio must be positive");
            }
        }
    }

    Ok(())
}

fn validate_headers(headers: &csv::StringRecord) -> anyhow::Result<()> {
    let actual: Vec<&str> = headers.iter().map(str::trim).collect();
    if actual != EXPECTED_HEADERS {
        bail!(
            "transaction CSV must use classified schema header: {}",
            EXPECTED_HEADERS.join(",")
        );
    }
    Ok(())
}

fn parse_optional(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_optional_enum<E>(s: &str, row_num: usize, field: &str) -> anyhow::Result<Option<E>>
where
    E: ValueEnum,
{
    parse_optional(s)
        .map(|value| {
            E::from_str(&value, true)
                .map_err(|_| anyhow::anyhow!("row {row_num}: invalid {field} '{value}'"))
        })
        .transpose()
}
