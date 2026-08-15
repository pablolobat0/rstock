use std::collections::{HashMap, HashSet};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use clap::ValueEnum;
use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::constants::{format_date, DISPLAY_DATE_FORMAT, FLOAT_EPSILON};
use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
    transaction_repo,
};
use crate::models::{
    f64_to_cents, Asset, AssetClass, AssetClassification, AssetInfo, AssetType, BondCredit,
    BondDuration, BuyOrder, CsvRow, DividendOrder, EquityStyle, Management, SellOrder, SplitOrder,
    Transaction, TxType,
};
use crate::services::transactions;

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

#[derive(Debug)]
pub struct ImportResult {
    pub count: usize,
    pub transaction_receipts: Vec<transactions::TransactionReceipt>,
}

#[allow(clippy::too_many_lines)]
pub async fn import_transactions_csv(
    db: &DatabaseConnection,
    path: &str,
) -> anyhow::Result<ImportResult> {
    let mut rows = read_rows(path)?;
    rows.sort_by_key(|r| r.date);

    let count = rows.len();
    if rows.is_empty() {
        return Ok(ImportResult {
            count,
            transaction_receipts: Vec::new(),
        });
    }

    let mutation = db.begin().await?;
    let existing_assets = asset_repo::find_all(&mutation).await?;
    let existing_transactions =
        transaction_repo::find_all_ordered_by_date(&mutation, None, None).await?;
    let transactions_by_asset = existing_transactions.into_iter().fold(
        HashMap::<i32, Vec<Transaction>>::new(),
        |mut transactions, transaction| {
            transactions
                .entry(transaction.asset_id)
                .or_default()
                .push(transaction);
            transactions
        },
    );

    let mut assets_by_ticker = existing_assets
        .into_iter()
        .map(|asset| {
            let asset_id = asset.id;
            let state = ImportAsset::from_existing(
                asset,
                transactions_by_asset
                    .get(&asset_id)
                    .cloned()
                    .unwrap_or_default(),
            );
            (state.ticker.clone(), state)
        })
        .collect::<HashMap<_, _>>();
    let mut pending_assets = Vec::new();
    let mut summaries = Vec::with_capacity(count);
    let mut split_tickers = HashSet::new();
    let mut invalidation_date = None;

    for row in &rows {
        let row_num = row.source_row;
        let date_str = format_date(row.date);

        if !assets_by_ticker.contains_key(&row.ticker) {
            let (name, asset_type, currency) = match row.tx_type {
                TxType::Buy => {
                    let name = row
                        .name
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "row {row_num}: buy transaction requires a non-empty Name"
                            )
                        })?;
                    let asset_type = row.asset_type.clone().ok_or_else(|| {
                        anyhow::anyhow!(
                            "row {row_num}: buy transaction requires a non-empty AssetType"
                        )
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
                    row.classification
                        .validate_for_asset(&asset_type, row.morningstar_code.as_deref())
                        .map_err(|e| anyhow::anyhow!("row {row_num}: {e}"))?;
                    (name.to_owned(), asset_type, currency.to_owned())
                }
                TxType::Sell | TxType::Dividend | TxType::Split => {
                    anyhow::bail!(
                        "row {row_num}: Asset with ticker '{}' not found",
                        row.ticker
                    )
                }
            };

            let pending_index = pending_assets.len();
            pending_assets.push(asset_repo::AssetWrite {
                info: AssetInfo {
                    ticker: row.ticker.clone(),
                    name: name.clone(),
                    asset_type: asset_type.clone(),
                    currency: currency.clone(),
                },
                classification: row.classification.clone(),
                morningstar_code: row.morningstar_code.clone(),
            });
            let temporary_id =
                -i32::try_from(pending_index + 1).expect("pending asset count fits in i32");
            assets_by_ticker.insert(
                row.ticker.clone(),
                ImportAsset::new_pending(
                    temporary_id,
                    row.ticker.clone(),
                    name,
                    currency,
                    pending_index,
                ),
            );
        }

        let state = assets_by_ticker
            .get_mut(&row.ticker)
            .expect("asset was resolved before validation");
        state.advance_holdings(&date_str);
        let row_invalidation_date = if row.tx_type == TxType::Split {
            state
                .first_transaction_date
                .clone()
                .unwrap_or_else(|| date_str.clone())
        } else {
            date_str.clone()
        };

        match row.tx_type {
            TxType::Buy => {
                let total = row.quantity * row.price + row.fees;
                summaries.push(format!(
                    "Bought {} units of {} ({}) at {:.2} {} on {}. Total: {:.2} {}",
                    row.quantity,
                    state.name,
                    state.ticker,
                    row.price,
                    state.currency,
                    crate::constants::display_date(&date_str),
                    total,
                    state.currency
                ));
            }
            TxType::Sell => {
                if row.quantity > state.holdings + FLOAT_EPSILON {
                    anyhow::bail!(
                        "row {row_num}: Insufficient holdings: you have {:.4} units of {} but tried to sell {:.4}",
                        state.holdings,
                        row.ticker,
                        row.quantity
                    );
                }
                let proceeds = row.quantity * row.price - row.fees;
                summaries.push(format!(
                    "Sold {} units of {} ({}) at {:.2} on {}. Proceeds: {:.2}",
                    row.quantity,
                    state.name,
                    state.ticker,
                    row.price,
                    crate::constants::display_date(&date_str),
                    proceeds
                ));
            }
            TxType::Dividend => {
                if state.holdings <= FLOAT_EPSILON {
                    anyhow::bail!(
                        "row {row_num}: No holdings of {} at date {}",
                        row.ticker,
                        crate::constants::display_date(&date_str)
                    );
                }
                let net_amount = row.price - row.fees;
                summaries.push(format!(
                    "Dividend for {} ({}): {:.2} (fees: {:.2}, net: {:.2}) on {}",
                    state.name,
                    row.ticker,
                    row.price,
                    row.fees,
                    net_amount,
                    crate::constants::display_date(&date_str)
                ));
            }
            TxType::Split => {
                if state.holdings <= FLOAT_EPSILON {
                    anyhow::bail!(
                        "row {row_num}: No holdings of {} at date {}",
                        row.ticker,
                        crate::constants::display_date(&date_str)
                    );
                }
                let post_split_qty = state.holdings * row.quantity;
                summaries.push(format!(
                    "Split {} ({}): ratio {}, holdings {:.4} -> {:.4} on {}",
                    state.name,
                    row.ticker,
                    row.quantity,
                    state.holdings,
                    post_split_qty,
                    crate::constants::display_date(&date_str)
                ));
                split_tickers.insert(row.ticker.clone());
            }
        }

        let transaction = transaction_for_row(row, state.id, &date_str);
        state.apply_imported_transaction(&transaction);
        if invalidation_date
            .as_ref()
            .is_none_or(|current| row_invalidation_date < *current)
        {
            invalidation_date = Some(row_invalidation_date);
        }
    }

    asset_repo::create_many(&mutation, &pending_assets).await?;
    let persisted_assets = asset_repo::find_all(&mutation).await?;
    let ids_by_ticker = persisted_assets
        .into_iter()
        .map(|asset| (asset.ticker, asset.id))
        .collect::<HashMap<_, _>>();
    for state in assets_by_ticker.values_mut() {
        if state.pending_asset.is_some() {
            state.id = *ids_by_ticker
                .get(&state.ticker)
                .ok_or_else(|| anyhow::anyhow!("asset '{}' was not persisted", state.ticker))?;
        }
    }

    let writes = rows
        .iter()
        .map(|row| {
            let asset_id = assets_by_ticker
                .get(&row.ticker)
                .expect("asset was resolved before persistence")
                .id;
            transaction_write_for_row(row, asset_id)
        })
        .collect::<Vec<_>>();
    let transaction_ids = transaction_repo::insert_many(&mutation, &writes).await?;
    if transaction_ids.len() != rows.len() {
        anyhow::bail!(
            "bulk transaction insert returned {} IDs for {} rows",
            transaction_ids.len(),
            rows.len()
        );
    }

    let split_asset_ids = split_tickers.into_iter().map(|ticker| {
        assets_by_ticker
            .get(&ticker)
            .expect("split asset was resolved before persistence")
            .id
    });
    daily_price_repo::delete_all_for_assets(&mutation, split_asset_ids).await?;
    if let Some(date) = invalidation_date {
        portfolio_history_repo::delete_from_date(&mutation, &date).await?;
        portfolio_asset_history_repo::delete_from_date(&mutation, &date).await?;
    }
    mutation.commit().await?;

    tracing::info!(count, "transaction CSV imported atomically");
    Ok(ImportResult {
        count,
        transaction_receipts: transaction_ids
            .into_iter()
            .zip(summaries)
            .map(
                |(transaction_id, summary)| transactions::TransactionReceipt {
                    transaction_id,
                    summary,
                },
            )
            .collect(),
    })
}

struct ImportAsset {
    id: i32,
    ticker: String,
    name: String,
    currency: String,
    pending_asset: Option<usize>,
    existing_transactions: Vec<Transaction>,
    existing_cursor: usize,
    holdings: f64,
    first_transaction_date: Option<String>,
}

impl ImportAsset {
    fn from_existing(asset: Asset, existing_transactions: Vec<Transaction>) -> Self {
        Self {
            id: asset.id,
            ticker: asset.ticker,
            name: asset.name,
            currency: asset.currency,
            pending_asset: None,
            first_transaction_date: existing_transactions.first().map(|tx| tx.date.clone()),
            existing_transactions,
            existing_cursor: 0,
            holdings: 0.0,
        }
    }

    fn new_pending(
        id: i32,
        ticker: String,
        name: String,
        currency: String,
        pending_asset: usize,
    ) -> Self {
        Self {
            id,
            ticker,
            name,
            currency,
            pending_asset: Some(pending_asset),
            existing_transactions: Vec::new(),
            existing_cursor: 0,
            holdings: 0.0,
            first_transaction_date: None,
        }
    }

    fn advance_holdings(&mut self, date: &str) {
        while self
            .existing_transactions
            .get(self.existing_cursor)
            .is_some_and(|transaction| transaction.date.as_str() <= date)
        {
            let transaction = &self.existing_transactions[self.existing_cursor];
            apply_transaction(
                &mut self.holdings,
                &transaction.tx_type,
                transaction.quantity,
            );
            self.existing_cursor += 1;
        }
    }

    fn apply_imported_transaction(&mut self, transaction: &Transaction) {
        apply_transaction(
            &mut self.holdings,
            &transaction.tx_type,
            transaction.quantity,
        );
        if self
            .first_transaction_date
            .as_ref()
            .is_none_or(|date| transaction.date < *date)
        {
            self.first_transaction_date = Some(transaction.date.clone());
        }
    }
}

fn apply_transaction(holdings: &mut f64, tx_type: &TxType, quantity: f64) {
    match tx_type {
        TxType::Buy => *holdings += quantity,
        TxType::Sell => *holdings -= quantity,
        TxType::Dividend => {}
        TxType::Split => *holdings *= quantity,
    }
}

fn transaction_for_row(row: &CsvRow, asset_id: i32, date: &str) -> Transaction {
    let (quantity, price, fees) = match row.tx_type {
        TxType::Buy | TxType::Sell => (row.quantity, row.price, row.fees),
        TxType::Dividend => (1.0, row.price, row.fees),
        TxType::Split => (row.quantity, 0.0, 0.0),
    };
    Transaction {
        id: 0,
        asset_id,
        tx_type: row.tx_type.clone(),
        date: date.to_owned(),
        quantity,
        price_cents: f64_to_cents(price),
        fees_cents: f64_to_cents(fees),
    }
}

fn transaction_write_for_row(row: &CsvRow, asset_id: i32) -> transaction_repo::TransactionWrite {
    let date = format_date(row.date);
    match row.tx_type {
        TxType::Buy => transaction_repo::TransactionWrite::Buy {
            asset_id,
            order: BuyOrder {
                date,
                quantity: row.quantity,
                price: row.price,
                fees: row.fees,
            },
        },
        TxType::Sell => transaction_repo::TransactionWrite::Sell {
            asset_id,
            order: SellOrder {
                date,
                quantity: row.quantity,
                price: row.price,
                fees: row.fees,
            },
        },
        TxType::Dividend => transaction_repo::TransactionWrite::Dividend {
            asset_id,
            order: DividendOrder {
                date,
                amount: row.price,
                fees: row.fees,
            },
        },
        TxType::Split => transaction_repo::TransactionWrite::Split {
            asset_id,
            order: SplitOrder {
                date,
                ratio: row.quantity,
            },
        },
    }
}

fn read_rows(path: &str) -> anyhow::Result<Vec<CsvRow>> {
    let mut reader =
        csv::Reader::from_path(path).with_context(|| format!("failed to open CSV file: {path}"))?;
    validate_headers(reader.headers()?)?;

    reader
        .records()
        .enumerate()
        .map(|(index, result)| {
            let row_num = index + 2;
            let record =
                result.with_context(|| format!("row {row_num}: failed to read CSV record"))?;
            parse_row(&record, row_num)
        })
        .collect()
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
