use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use clap::ValueEnum;
use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::constants::{format_date, DISPLAY_DATE_FORMAT};
use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
    transaction_repo,
};
use crate::models::{
    cents_are_representable, cents_to_f64, Asset, AssetClass, AssetClassification, AssetInfo,
    AssetType, BondCredit, BondDuration, BuyOrder, CsvRow, DividendOrder, EquityStyle, Management,
    SellOrder, SplitOrder, Transaction, TxType,
};
use crate::services::ledger::{self, LedgerEffect, LedgerEntryKind, LedgerTransition};
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
    // Generated IDs follow this write order.  Include the source row explicitly
    // so same-date entries retain CSV order even if the sort implementation's
    // stability ever changes.
    rows.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then(left.source_row.cmp(&right.source_row))
    });

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
                    .map_or(&[][..], Vec::as_slice),
            );
            (state.ticker.clone(), state)
        })
        .collect::<HashMap<_, _>>();
    let mut pending_assets = Vec::new();
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
            let temporary_id = -i32::try_from(pending_index + 1)
                .context("pending asset count exceeds the supported asset ID range")?;
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

        let state = assets_by_ticker.get_mut(&row.ticker).ok_or_else(|| {
            anyhow::anyhow!("row {row_num}: asset '{}' was not resolved", row.ticker)
        })?;
        let row_invalidation_date = if row.tx_type == TxType::Split {
            state
                .first_transaction_date
                .clone()
                .unwrap_or_else(|| date_str.clone())
        } else {
            date_str.clone()
        };

        if row.tx_type == TxType::Split {
            split_tickers.insert(row.ticker.clone());
        }
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
    let tickers_by_asset_id = assets_by_ticker
        .values()
        .map(|asset| (asset.id, asset.ticker.as_str()))
        .collect::<HashMap<_, _>>();

    let writes = rows
        .iter()
        .map(|row| {
            let asset_id = assets_by_ticker
                .get(&row.ticker)
                .ok_or_else(|| anyhow::anyhow!("asset '{}' was not resolved", row.ticker))?
                .id;
            Ok(transaction_write_for_row(row, asset_id))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let transaction_ids = transaction_repo::insert_many(&mutation, &writes).await?;
    if transaction_ids.len() != rows.len() {
        anyhow::bail!(
            "bulk transaction insert returned {} IDs for {} rows",
            transaction_ids.len(),
            rows.len()
        );
    }

    let source_rows_by_transaction_id = transaction_ids
        .iter()
        .copied()
        .zip(rows.iter().map(|row| row.source_row))
        .collect::<HashMap<_, _>>();
    let mut source_rows_by_asset = HashMap::<i32, Vec<usize>>::new();
    for row in &rows {
        let asset_id = assets_by_ticker
            .get(&row.ticker)
            .ok_or_else(|| anyhow::anyhow!("asset '{}' was not resolved", row.ticker))?
            .id;
        source_rows_by_asset
            .entry(asset_id)
            .or_default()
            .push(row.source_row);
    }

    let affected_asset_ids: BTreeSet<i32> = rows
        .iter()
        .filter_map(|row| assets_by_ticker.get(&row.ticker).map(|asset| asset.id))
        .collect();
    let mut imported_transitions = HashMap::<i32, LedgerTransition>::with_capacity(count);
    for asset_id in affected_asset_ids {
        let transactions = transaction_repo::find_by_asset_id(&mutation, asset_id).await?;
        match ledger::replay_transactions(asset_id, &transactions) {
            Ok(replay) => {
                for transition in replay.transitions {
                    if source_rows_by_transaction_id.contains_key(&transition.entry.id) {
                        imported_transitions.insert(transition.entry.id, transition);
                    }
                }
            }
            Err(error) => {
                let source_row = source_row_for_replay_error(
                    &error,
                    &transactions,
                    &source_rows_by_transaction_id,
                    source_rows_by_asset.get(&asset_id).map(Vec::as_slice),
                );
                if let Some(source_row) = source_row {
                    let ticker = tickers_by_asset_id
                        .get(&asset_id)
                        .copied()
                        .unwrap_or("unknown");
                    anyhow::bail!(
                        "row {source_row}: transaction CSV import replay failed for asset {ticker} ({asset_id}): {error}"
                    );
                }
                return Err(
                    anyhow::Error::from(error).context("transaction CSV import replay failed")
                );
            }
        }
    }

    let transaction_receipts = transaction_ids
        .iter()
        .copied()
        .zip(&rows)
        .map(|(transaction_id, row)| {
            let asset = assets_by_ticker
                .get(&row.ticker)
                .ok_or_else(|| anyhow::anyhow!("asset '{}' was not resolved", row.ticker))?;
            let transition = imported_transitions.get(&transaction_id).ok_or_else(|| {
                anyhow::anyhow!("imported transaction {transaction_id} has no replay transition")
            })?;
            Ok(transactions::TransactionReceipt {
                transaction_id,
                summary: import_summary(asset, transition)?,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

    let split_asset_ids = split_tickers
        .into_iter()
        .map(|ticker| {
            assets_by_ticker
                .get(&ticker)
                .ok_or_else(|| anyhow::anyhow!("asset '{ticker}' was not resolved"))
                .map(|asset| asset.id)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    daily_price_repo::delete_all_for_assets(&mutation, split_asset_ids).await?;
    if let Some(date) = invalidation_date {
        portfolio_history_repo::delete_from_date(&mutation, &date).await?;
        portfolio_asset_history_repo::delete_from_date(&mutation, &date).await?;
    }
    mutation.commit().await?;

    tracing::info!(count, "transaction CSV imported atomically");
    Ok(ImportResult {
        count,
        transaction_receipts,
    })
}

struct ImportAsset {
    id: i32,
    ticker: String,
    name: String,
    currency: String,
    pending_asset: Option<usize>,
    first_transaction_date: Option<String>,
}

impl ImportAsset {
    fn from_existing(asset: Asset, existing_transactions: &[Transaction]) -> Self {
        Self {
            id: asset.id,
            ticker: asset.ticker,
            name: asset.name,
            currency: asset.currency,
            pending_asset: None,
            first_transaction_date: existing_transactions.first().map(|tx| tx.date.clone()),
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
            first_transaction_date: None,
        }
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
    if !quantity.is_finite() || !price.is_finite() || !fees.is_finite() {
        bail!("row {row_num}: numeric values must be finite");
    }
    if !matches!(tx_type, TxType::Split)
        && (!cents_are_representable(price) || !cents_are_representable(fees))
    {
        bail!("row {row_num}: monetary values exceed supported precision");
    }
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
            if fees > price {
                bail!("row {row_num}: dividend deductions must not exceed gross amount");
            }
        }
        TxType::Split => {
            if quantity <= 0.0 {
                bail!("row {row_num}: split ratio must be positive");
            }
            if price != 0.0 || fees != 0.0 {
                bail!("row {row_num}: split Price and Fees placeholders must both be zero");
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

fn import_summary(asset: &ImportAsset, transition: &LedgerTransition) -> anyhow::Result<String> {
    let date = crate::constants::display_date(&transition.entry.date);
    match (&transition.entry.kind, &transition.effect) {
        (
            LedgerEntryKind::Buy {
                units,
                unit_price_cents,
                ..
            },
            LedgerEffect::Buy { contribution },
        ) => Ok(format!(
            "Bought {} units of {} ({}) at {:.2} {} on {}. Total: {:.2} {}",
            units,
            asset.name,
            asset.ticker,
            cents_to_f64(*unit_price_cents),
            asset.currency,
            date,
            contribution / crate::constants::MONETARY_MULTIPLIER,
            asset.currency
        )),
        (
            LedgerEntryKind::Sell {
                units,
                unit_price_cents,
                ..
            },
            LedgerEffect::Sell { withdrawal, .. },
        ) => Ok(format!(
            "Sold {} units of {} ({}) at {:.2} on {}. Proceeds: {:.2}",
            units,
            asset.name,
            asset.ticker,
            cents_to_f64(*unit_price_cents),
            date,
            withdrawal / crate::constants::MONETARY_MULTIPLIER
        )),
        (
            LedgerEntryKind::Dividend {
                gross_amount_cents,
                deductions_cents,
            },
            LedgerEffect::Dividend { net_income },
        ) => Ok(format!(
            "Dividend for {} ({}): {:.2} (fees: {:.2}, net: {:.2}) on {}",
            asset.name,
            asset.ticker,
            cents_to_f64(*gross_amount_cents),
            cents_to_f64(*deductions_cents),
            net_income / crate::constants::MONETARY_MULTIPLIER,
            date
        )),
        (LedgerEntryKind::Split { ratio }, LedgerEffect::Split { .. }) => Ok(format!(
            "Split {} ({}): ratio {} on {}",
            asset.name, asset.ticker, ratio, date
        )),
        _ => bail!(
            "imported transaction {} has inconsistent replay semantics",
            transition.entry.id
        ),
    }
}

fn source_row_for_replay_error(
    error: &ledger::LedgerError,
    transactions: &[Transaction],
    source_rows_by_transaction_id: &HashMap<i32, usize>,
    source_rows: Option<&[usize]>,
) -> Option<usize> {
    source_rows_by_transaction_id
        .get(&error.entry_id)
        .copied()
        .or_else(|| {
            transactions
                .iter()
                .position(|transaction| transaction.id == error.entry_id)
                .and_then(|error_index| {
                    transactions[..error_index]
                        .iter()
                        .rev()
                        .filter(|transaction| {
                            matches!(
                                &transaction.tx_type,
                                TxType::Buy | TxType::Sell | TxType::Split
                            )
                        })
                        .find_map(|transaction| source_rows_by_transaction_id.get(&transaction.id))
                        .copied()
                })
        })
        .or_else(|| source_rows.and_then(|rows| rows.first().copied()))
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
