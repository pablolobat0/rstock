use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::Context;
use chrono::{Duration, NaiveDate};
use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::constants::{format_date, FLOAT_EPSILON, INITIAL_NAV};
use crate::db::repos::{
    asset_repo, portfolio_asset_history_repo, portfolio_history_repo, transaction_repo,
};
use crate::models::{
    cents_to_f64, Asset, AssetSnapshot, MarketDataLimitation, PortfolioSnapshot, Transaction,
};
use crate::services::market_data::{MarketData, NavValuationData};

/// NAV history made ready for consumers, together with the limitations that
/// bound the resulting historical valuation scope.
#[derive(Debug)]
pub struct PortfolioHistoryReadiness {
    pub latest_snapshot: Option<PortfolioSnapshot>,
    pub market_data_limitations: Vec<MarketDataLimitation>,
    pub(crate) performance_market_data_prepared: bool,
}

struct NavMarketDataPreparation {
    effective_end: NaiveDate,
    limitations: Vec<MarketDataLimitation>,
    data_available: bool,
    holdings: HashMap<i32, f64>,
    transactions: Vec<Transaction>,
    assets: Vec<Asset>,
    valuation_data: Option<NavValuationData>,
}

struct SnapshotBatch {
    portfolio_snapshots: Vec<PortfolioSnapshot>,
    asset_snapshots: Vec<AssetSnapshot>,
}

const SNAPSHOT_BATCH_SIZE: usize = 100;

/// Reads an already-ready history range without performing readiness work.
pub async fn get_ready_portfolio_history(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<PortfolioSnapshot>> {
    portfolio_history_repo::find_between(db, start_date, end_date).await
}

/// Ensures portfolio history is ready through the Effective valuation date
/// supported by Historical market data for the latest completed date.
///
/// Unavailable required market data for a currently held performance asset is a
/// normal outcome: it is represented as a readiness with no latest snapshot and
/// NAV-scoped `Market data limitation` values rather than a hard error. Only
/// genuine failures (DB, date parsing, missing Morningstar code, invariants)
/// propagate as errors.
pub async fn ensure_portfolio_history(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<PortfolioHistoryReadiness> {
    let yesterday = market_data.today() - Duration::days(1);
    let yesterday_str = format_date(yesterday);

    let mut latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    let incomplete_date = match latest_snapshot.as_ref() {
        Some(snapshot) => {
            let cached = market_data
                .nav_completeness_audits
                .lock()
                .map_err(|_| anyhow::anyhow!("NAV completeness cache was poisoned"))?
                .get(&snapshot.date)
                .cloned();
            if let Some(result) = cached {
                result
            } else {
                let result = find_first_incomplete_snapshot(db, &snapshot.date).await?;
                if result.is_none() {
                    market_data
                        .nav_completeness_audits
                        .lock()
                        .map_err(|_| anyhow::anyhow!("NAV completeness cache was poisoned"))?
                        .insert(snapshot.date.clone(), None);
                }
                result
            }
        }
        None => None,
    };
    if let Some(incomplete_date) = incomplete_date {
        tracing::warn!(date = %incomplete_date, "discarding incomplete NAV snapshots");
        discard_incomplete_snapshots_from(db, &incomplete_date).await?;
        latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    }

    let mut market_data_limitations = Vec::new();
    let mut performance_market_data_prepared = false;
    match &latest_snapshot {
        Some(snapshot) if snapshot.date >= yesterday_str => {}
        Some(snapshot) => {
            let latest_date =
                NaiveDate::parse_from_str(&snapshot.date, crate::constants::DATE_FORMAT)
                    .context("invalid latest snapshot date")?;
            let start = latest_date + Duration::days(1);
            let preparation = nav_market_data_availability(
                db,
                market_data,
                start,
                yesterday,
                Some(snapshot),
                None,
            )
            .await?;
            performance_market_data_prepared = true;
            let limitations = preparation.limitations.clone();
            if preparation.data_available {
                rebuild_portfolio_history(db, start, yesterday, preparation, Some(snapshot))
                    .await?;
            }
            market_data_limitations = limitations;
        }
        None => {
            let transactions =
                transaction_repo::find_all_ordered_by_date(db, None, Some(&yesterday_str)).await?;
            if let Some(transaction) = transactions.first() {
                let start =
                    NaiveDate::parse_from_str(&transaction.date, crate::constants::DATE_FORMAT)
                        .context("invalid first transaction date")?;
                let preparation = nav_market_data_availability(
                    db,
                    market_data,
                    start,
                    yesterday,
                    None,
                    Some(transactions),
                )
                .await?;
                performance_market_data_prepared = true;
                let limitations = preparation.limitations.clone();
                if preparation.data_available {
                    rebuild_portfolio_history(db, start, yesterday, preparation, None).await?;
                }
                market_data_limitations = limitations;
            }
        }
    }

    Ok(PortfolioHistoryReadiness {
        latest_snapshot: portfolio_history_repo::find_latest(db).await?,
        market_data_limitations,
        performance_market_data_prepared,
    })
}

async fn find_first_incomplete_snapshot(
    db: &DatabaseConnection,
    latest_date: &str,
) -> anyhow::Result<Option<String>> {
    let snapshot_dates = portfolio_history_repo::find_dates_between(db, "", latest_date).await?;
    let transactions = transaction_repo::find_holdings_inputs(db, Some(latest_date)).await?;
    let asset_ids: HashSet<i32> = transactions
        .iter()
        .map(|transaction| transaction.asset_id)
        .collect();
    let assets = asset_repo::find_by_ids(db, asset_ids.iter().copied()).await?;
    let asset_map: HashMap<i32, &Asset> = assets.iter().map(|asset| (asset.id, asset)).collect();
    let mut holdings = HashMap::<i32, f64>::new();
    let mut expected_asset_ids = BTreeSet::new();
    let mut expected_asset_ids_string = String::new();
    let mut expected_asset_ids_dirty = true;
    let mut expected_asset_ids_by_date = Vec::<(String, String)>::new();
    let mut transaction_index = 0;
    for date in snapshot_dates {
        while transaction_index < transactions.len() && transactions[transaction_index].date <= date
        {
            let transaction = &transactions[transaction_index];
            expected_asset_ids_dirty |= apply_transaction_to_holdings(
                &mut holdings,
                &mut expected_asset_ids,
                transaction,
                &asset_map,
            );
            transaction_index += 1;
        }

        if expected_asset_ids_dirty {
            expected_asset_ids_string = format_asset_ids(&expected_asset_ids);
            expected_asset_ids_dirty = false;
        }
        expected_asset_ids_by_date.push((date, expected_asset_ids_string.clone()));
    }

    let actual_asset_ids_by_date = portfolio_asset_history_repo::find_all(db)
        .await?
        .into_iter()
        .fold(
            HashMap::<String, BTreeSet<i32>>::new(),
            |mut snapshots, snapshot| {
                snapshots
                    .entry(snapshot.date)
                    .or_default()
                    .insert(snapshot.asset_id);
                snapshots
            },
        );
    for (date, expected_asset_ids) in expected_asset_ids_by_date {
        let expected_asset_ids = parse_asset_ids(&expected_asset_ids)?;
        let actual_asset_ids = actual_asset_ids_by_date
            .get(&date)
            .cloned()
            .unwrap_or_default();
        if !expected_asset_ids.is_subset(&actual_asset_ids) {
            return Ok(Some(date));
        }
    }

    Ok(None)
}

fn apply_transaction_to_holdings(
    holdings: &mut HashMap<i32, f64>,
    expected_asset_ids: &mut BTreeSet<i32>,
    transaction: &Transaction,
    asset_map: &HashMap<i32, &Asset>,
) -> bool {
    if asset_map
        .get(&transaction.asset_id)
        .is_some_and(|asset| !asset.is_monetary())
    {
        let holding = holdings.entry(transaction.asset_id).or_default();
        if transaction.is_split() {
            *holding *= transaction.quantity;
        } else if transaction.is_buy() {
            *holding += transaction.quantity;
        } else if transaction.is_sell() {
            *holding -= transaction.quantity;
        }
        let was_expected = expected_asset_ids.contains(&transaction.asset_id);
        let is_expected = *holding > FLOAT_EPSILON;
        if is_expected {
            expected_asset_ids.insert(transaction.asset_id);
        } else {
            expected_asset_ids.remove(&transaction.asset_id);
        }
        return was_expected != is_expected;
    }
    false
}

fn format_asset_ids(asset_ids: &BTreeSet<i32>) -> String {
    let mut formatted = String::new();
    for (index, asset_id) in asset_ids.iter().enumerate() {
        if index > 0 {
            formatted.push(',');
        }
        formatted.push_str(&asset_id.to_string());
    }
    formatted
}

fn parse_asset_ids(formatted: &str) -> anyhow::Result<BTreeSet<i32>> {
    formatted
        .split(',')
        .filter(|asset_id| !asset_id.is_empty())
        .map(|asset_id| asset_id.parse().context("invalid asset snapshot ID"))
        .collect()
}

async fn discard_incomplete_snapshots_from(
    db: &DatabaseConnection,
    date: &str,
) -> anyhow::Result<()> {
    let transaction = db.begin().await?;
    portfolio_history_repo::delete_from_date(&transaction, date).await?;
    portfolio_asset_history_repo::delete_from_date(&transaction, date).await?;
    transaction.commit().await?;
    Ok(())
}

/// Prepares valuation market data once for the holdings that must be valued
/// across `[start, end]` and reports whether every required asset price and FX
/// rate is available. The single preparation pass both fills the cache the
/// rebuild will reuse and yields the NAV-scoped limitations; no second pass is
/// needed to reconstruct them after a failed rebuild.
async fn nav_market_data_availability(
    db: &DatabaseConnection,
    market_data: &MarketData,
    start: NaiveDate,
    end: NaiveDate,
    prev_snapshot: Option<&PortfolioSnapshot>,
    prepared_transactions: Option<Vec<Transaction>>,
) -> anyhow::Result<NavMarketDataPreparation> {
    let start_str = format_date(start);
    let end_str = format_date(end);
    let mut holdings: HashMap<i32, f64> = HashMap::new();
    if let Some(snapshot) = prev_snapshot {
        let asset_rows = portfolio_asset_history_repo::find_by_date(db, &snapshot.date).await?;
        for row in asset_rows {
            holdings.insert(row.asset_id, row.quantity);
        }
    }
    let transactions = match prepared_transactions {
        Some(transactions) => transactions,
        None => {
            transaction_repo::find_all_ordered_by_date(db, Some(&start_str), Some(&end_str)).await?
        }
    };
    let needed_ids: HashSet<i32> = holdings
        .keys()
        .copied()
        .chain(transactions.iter().map(|tx| tx.asset_id))
        .collect();
    if needed_ids.is_empty() {
        return Ok(NavMarketDataPreparation {
            effective_end: end,
            limitations: Vec::new(),
            data_available: true,
            holdings,
            transactions,
            assets: Vec::new(),
            valuation_data: None,
        });
    }
    let assets = asset_repo::find_by_ids(db, needed_ids).await?;
    let nav_assets: Vec<Asset> = assets
        .iter()
        .filter(|asset| !asset.is_monetary())
        .cloned()
        .collect();
    let (mut availability, valuation_data) = market_data
        .prepare_valuation_market_data_for_nav(db, &nav_assets, &start_str, &end_str)
        .await?;
    if availability.data_available {
        let mut first_valuation_dates: HashMap<i32, NaiveDate> = holdings
            .iter()
            .filter(|(_, quantity)| **quantity > FLOAT_EPSILON)
            .map(|(asset_id, _)| (*asset_id, start))
            .collect();
        for transaction in transactions
            .iter()
            .filter(|transaction| transaction.is_buy())
        {
            let transaction_date =
                NaiveDate::parse_from_str(&transaction.date, crate::constants::DATE_FORMAT)
                    .context("invalid transaction date")?;
            first_valuation_dates
                .entry(transaction.asset_id)
                .or_insert(transaction_date);
        }

        for asset in &nav_assets {
            let Some(first_valuation_date) = first_valuation_dates.get(&asset.id) else {
                continue;
            };
            let limitations = valuation_data.valuation_limitations(asset, *first_valuation_date);
            if !limitations.is_empty() {
                availability.data_available = false;
                for limitation in limitations {
                    if !availability.limitations.contains(&limitation) {
                        availability.limitations.push(limitation);
                    }
                }
            }
        }
    }
    let valuation_data = if availability.data_available {
        Some(valuation_data)
    } else {
        None
    };

    Ok(NavMarketDataPreparation {
        effective_end: availability.effective_end,
        limitations: availability.limitations,
        data_available: availability.data_available,
        holdings,
        transactions,
        assets,
        valuation_data,
    })
}

#[allow(clippy::too_many_lines)]
async fn rebuild_portfolio_history(
    db: &DatabaseConnection,
    start_date: NaiveDate,
    end_date: NaiveDate,
    preparation: NavMarketDataPreparation,
    prev_snapshot: Option<&PortfolioSnapshot>,
) -> anyhow::Result<()> {
    let NavMarketDataPreparation {
        effective_end,
        mut holdings,
        transactions,
        assets,
        valuation_data,
        ..
    } = preparation;
    tracing::info!(%start_date, %end_date, "rebuilding portfolio history");

    let mut is_fresh_portfolio = prev_snapshot.is_none();
    let mut outstanding_shares = prev_snapshot.map_or(0.0, |s| s.outstanding_shares);
    let mut nav = prev_snapshot.map_or(INITIAL_NAV, |s| s.nav);
    // Accumulated cash from dividends: recovered from total_value - asset_value
    let mut accumulated_cash = prev_snapshot.map_or(0.0, |s| s.total_value - s.asset_value);

    if assets.is_empty() {
        return Ok(());
    }
    let valuation_data = valuation_data.context("missing preloaded NAV valuation data")?;

    let mut tx_by_date: HashMap<String, Vec<&Transaction>> = HashMap::new();
    for tx in &transactions {
        tx_by_date.entry(tx.date.clone()).or_default().push(tx);
    }

    let asset_map: HashMap<i32, &Asset> = assets.iter().map(|a| (a.id, a)).collect();
    let mut snapshot_batch = SnapshotBatch {
        portfolio_snapshots: Vec::with_capacity(SNAPSHOT_BATCH_SIZE),
        asset_snapshots: Vec::new(),
    };

    // Iterate each calendar day
    let mut current = start_date;
    while current <= effective_end {
        let date_str = format_date(current);

        // Process transactions for this day
        if let Some(day_txs) = tx_by_date.get(&date_str) {
            let (new_shares, new_nav, dividend_income) = process_day_transactions(
                day_txs,
                &mut holdings,
                outstanding_shares,
                nav,
                &asset_map,
                &valuation_data,
                current,
            )?;
            outstanding_shares = new_shares;
            nav = new_nav;
            accumulated_cash += dividend_income;
        }

        if outstanding_shares == 0.0 && is_fresh_portfolio {
            current += chrono::Duration::days(1);
            continue;
        }

        // Compute EOD values (aggregate + per-asset) with currency conversion
        let (asset_value, asset_values) =
            compute_day_asset_values(&valuation_data, &holdings, &asset_map, &date_str, current)?;

        let total_value = asset_value + accumulated_cash;
        if outstanding_shares > 0.0 {
            nav = total_value / outstanding_shares;
        }

        // First-ever transaction day: store a seed snapshot only after required valuations succeed.
        if is_fresh_portfolio && outstanding_shares > 0.0 {
            let seed_date = format_date(current - chrono::Duration::days(1));
            snapshot_batch.portfolio_snapshots.push(PortfolioSnapshot {
                date: seed_date,
                asset_value: 0.0,
                total_value: 0.0,
                outstanding_shares: 0.0,
                nav: INITIAL_NAV,
            });
            is_fresh_portfolio = false;
        }

        snapshot_batch.portfolio_snapshots.push(PortfolioSnapshot {
            date: date_str,
            asset_value,
            total_value,
            outstanding_shares,
            nav,
        });
        snapshot_batch.asset_snapshots.extend(asset_values);

        if snapshot_batch.portfolio_snapshots.len() >= SNAPSHOT_BATCH_SIZE {
            persist_snapshot_batch(db, &mut snapshot_batch).await?;
        }

        current += chrono::Duration::days(1);
    }

    persist_snapshot_batch(db, &mut snapshot_batch).await?;

    Ok(())
}

/// Returns `(outstanding_shares, nav, dividend_income_eur)`.
#[allow(clippy::implicit_hasher)]
fn process_day_transactions(
    day_txs: &[&Transaction],
    holdings: &mut HashMap<i32, f64>,
    outstanding_shares: f64,
    nav: f64,
    asset_map: &HashMap<i32, &Asset>,
    valuation_data: &NavValuationData,
    date: NaiveDate,
) -> anyhow::Result<(f64, f64, f64)> {
    let mut os = outstanding_shares;
    let mut current_nav = nav;
    let mut dividend_income = 0.0;

    for tx in day_txs {
        if asset_map
            .get(&tx.asset_id)
            .is_some_and(|asset| asset.is_monetary())
        {
            continue;
        }

        // Convert to base currency
        let rate = asset_map
            .get(&tx.asset_id)
            .map(|asset| valuation_data.exchange_rate_for_asset(asset, date))
            .transpose()?
            .unwrap_or(1.0);

        if tx.is_split() {
            *holdings.entry(tx.asset_id).or_insert(0.0) *= tx.quantity;
        } else if tx.is_dividend() {
            // Dividend = income: accumulate as cash, no holdings or shares change
            let amount = tx.quantity * cents_to_f64(tx.price_cents) - cents_to_f64(tx.fees_cents);
            dividend_income += amount * rate;
        } else if tx.is_sell() {
            // Sell = withdrawal: proceeds = qty * price - fees
            let withdrawal =
                tx.quantity * cents_to_f64(tx.price_cents) - cents_to_f64(tx.fees_cents);
            let withdrawal_eur = withdrawal * rate;

            if os > 0.0 && current_nav > 0.0 {
                let shares_redeemed = withdrawal_eur / current_nav;
                os -= shares_redeemed;
                if os < 0.0 {
                    os = 0.0;
                }
            }

            *holdings.entry(tx.asset_id).or_insert(0.0) -= tx.quantity;
        } else {
            // Buy = deposit: cost = qty * price + fees
            let deposit = tx.quantity * cents_to_f64(tx.price_cents) + cents_to_f64(tx.fees_cents);
            let deposit_eur = deposit * rate;

            if os == 0.0 {
                current_nav = INITIAL_NAV;
                let shares_issued = deposit_eur / INITIAL_NAV;
                os = shares_issued;
            } else {
                let shares_issued = deposit_eur / current_nav;
                os += shares_issued;
            }

            *holdings.entry(tx.asset_id).or_insert(0.0) += tx.quantity;
        }
    }

    Ok((os, current_nav, dividend_income))
}

fn compute_day_asset_values(
    valuation_data: &NavValuationData,
    holdings: &HashMap<i32, f64>,
    asset_map: &HashMap<i32, &Asset>,
    date: &str,
    as_of: NaiveDate,
) -> anyhow::Result<(f64, Vec<AssetSnapshot>)> {
    let mut total_asset_value = 0.0;
    let mut asset_values = Vec::new();

    for (&asset_id, &qty) in holdings {
        if qty <= 0.0 {
            continue;
        }

        let Some(asset_model) = asset_map.get(&asset_id) else {
            continue;
        };
        if asset_model.is_monetary() {
            continue;
        }
        let valuation = valuation_data.valuation(asset_model, as_of)?;

        let market_value = qty * valuation.base_currency_price;
        total_asset_value += market_value;
        asset_values.push(AssetSnapshot {
            date: date.to_owned(),
            asset_id,
            quantity: qty,
            closing_price: valuation.native_price,
            market_value,
            exchange_rate: valuation.fx_rate,
        });
    }

    Ok((total_asset_value, asset_values))
}

async fn persist_snapshot_batch(
    db: &DatabaseConnection,
    batch: &mut SnapshotBatch,
) -> anyhow::Result<()> {
    if batch.portfolio_snapshots.is_empty() {
        return Ok(());
    }

    let transaction = db.begin().await?;
    portfolio_history_repo::upsert_many(&transaction, &batch.portfolio_snapshots).await?;
    portfolio_asset_history_repo::upsert_many(&transaction, &batch.asset_snapshots).await?;
    transaction.commit().await?;

    batch.portfolio_snapshots.clear();
    batch.asset_snapshots.clear();
    Ok(())
}
