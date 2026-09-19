use std::collections::{HashMap, HashSet};

use anyhow::Context;
use chrono::{Duration, NaiveDate};
use sea_orm::DatabaseConnection;
use tracing::Instrument;

use crate::constants::{format_date, FLOAT_EPSILON, INITIAL_NAV};
use crate::db::repos::{
    asset_repo, portfolio_asset_history_repo, portfolio_history_repo, transaction_repo,
    NavSnapshotSink,
};
use crate::models::{Asset, AssetSnapshot, MarketDataLimitation, PortfolioSnapshot, Transaction};
use crate::services::ledger::{self, EnrichedLedgerTransition, LedgerEffect, LedgerReplay};
use crate::services::market_data::{MarketData, NavValuationData, NavValuationInterval};

/// NAV history made ready for consumers, together with the limitations that
/// bound the resulting historical valuation scope.
#[derive(Debug)]
pub struct PortfolioHistoryReadiness {
    pub latest_snapshot: Option<PortfolioSnapshot>,
    pub market_data_limitations: Vec<MarketDataLimitation>,
    /// Repository reads performed while executing the prepared plan.
    pub execution_database_reads: usize,
    pub(crate) performance_market_data_prepared: bool,
}

struct NavRebuildPlan {
    start_date: NaiveDate,
    effective_end: NaiveDate,
    checkpoint: Option<PortfolioSnapshot>,
    holdings: HashMap<i32, f64>,
    transactions: Vec<EnrichedLedgerTransition>,
    assets: Vec<Asset>,
    valuation_data: NavValuationData,
    intervals: Vec<NavValuationInterval>,
    limitations: Vec<MarketDataLimitation>,
}

struct CompleteNavSnapshot {
    portfolio: PortfolioSnapshot,
    assets: Vec<AssetSnapshot>,
}

#[derive(Default)]
struct SnapshotBatch {
    snapshots: Vec<CompleteNavSnapshot>,
    generated_rows: usize,
}

const SNAPSHOT_BATCH_DATE_TARGET: usize = 100;
const SNAPSHOT_BATCH_ROW_TARGET: usize = 5_000;

/// Reads an already-ready history range without performing readiness work.
pub async fn get_ready_portfolio_history(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<PortfolioSnapshot>> {
    portfolio_history_repo::find_between(db, start_date, end_date).await
}

/// Ensures portfolio history is ready through the latest completed date that
/// can be reached by a contiguous prefix of prepared historical inputs.
pub async fn ensure_portfolio_history(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<PortfolioHistoryReadiness> {
    let yesterday = market_data.today() - Duration::days(1);
    let yesterday_str = format_date(yesterday);
    let checkpoint = portfolio_history_repo::find_latest(db).await?;

    let (start_date, checkpoint, prepared_transactions) = match checkpoint {
        Some(snapshot) if snapshot.date >= yesterday_str => {
            return Ok(PortfolioHistoryReadiness {
                latest_snapshot: Some(snapshot),
                market_data_limitations: Vec::new(),
                execution_database_reads: 0,
                performance_market_data_prepared: false,
            });
        }
        Some(snapshot) => {
            let date = NaiveDate::parse_from_str(&snapshot.date, crate::constants::DATE_FORMAT)
                .context("invalid latest snapshot date")?;
            (date + Duration::days(1), Some(snapshot), None)
        }
        None => {
            let transactions =
                transaction_repo::find_all_ordered_by_date(db, None, Some(&yesterday_str)).await?;
            let Some(first) = transactions.first() else {
                return Ok(PortfolioHistoryReadiness {
                    latest_snapshot: None,
                    market_data_limitations: Vec::new(),
                    execution_database_reads: 0,
                    performance_market_data_prepared: false,
                });
            };
            let date = NaiveDate::parse_from_str(&first.date, crate::constants::DATE_FORMAT)
                .context("invalid first transaction date")?;
            (date, None, Some(transactions))
        }
    };

    if start_date > yesterday {
        return Ok(PortfolioHistoryReadiness {
            latest_snapshot: checkpoint,
            market_data_limitations: Vec::new(),
            execution_database_reads: 0,
            performance_market_data_prepared: false,
        });
    }

    let plan = prepare_rebuild_plan(
        db,
        market_data,
        start_date,
        yesterday,
        checkpoint,
        prepared_transactions,
    )
    .await?;
    let limitations = plan.limitations.clone();
    let execution_sink = NavSnapshotSink::new(db);
    let (execution, execution_database_reads) = crate::db::repos::with_nav_execution_probe(
        execute_rebuild_plan(&execution_sink, &plan)
            .instrument(tracing::info_span!("nav_plan_execution")),
    )
    .await;
    execution?;

    let readiness = PortfolioHistoryReadiness {
        latest_snapshot: portfolio_history_repo::find_latest(db).await?,
        market_data_limitations: limitations,
        execution_database_reads,
        performance_market_data_prepared: true,
    };
    tracing::debug!(
        execution_database_reads = readiness.execution_database_reads,
        "prepared NAV execution completed"
    );
    Ok(readiness)
}

#[allow(clippy::too_many_lines)]
async fn prepare_rebuild_plan(
    db: &DatabaseConnection,
    market_data: &MarketData,
    start_date: NaiveDate,
    end_date: NaiveDate,
    checkpoint: Option<PortfolioSnapshot>,
    prepared_transactions: Option<Vec<Transaction>>,
) -> anyhow::Result<NavRebuildPlan> {
    let mut holdings = HashMap::new();
    if let Some(snapshot) = &checkpoint {
        for row in portfolio_asset_history_repo::find_by_date(db, &snapshot.date).await? {
            holdings.insert(row.asset_id, row.quantity);
        }
    }

    let end_date_str = format_date(end_date);
    let transaction_start = checkpoint.as_ref().map(|_| format_date(start_date));
    let persisted_transactions = match prepared_transactions {
        Some(transactions) => transactions,
        None => {
            transaction_repo::find_all_ordered_by_date(
                db,
                transaction_start.as_deref(),
                Some(&end_date_str),
            )
            .await?
        }
    };
    let mut transactions_by_asset = HashMap::<i32, Vec<Transaction>>::new();
    for transaction in persisted_transactions {
        transactions_by_asset
            .entry(transaction.asset_id)
            .or_default()
            .push(transaction);
    }

    let asset_ids: HashSet<i32> = holdings
        .keys()
        .copied()
        .chain(transactions_by_asset.keys().copied())
        .collect();
    let mut assets = asset_repo::find_by_ids(db, asset_ids).await?;
    assets.sort_by_key(|asset| asset.id);
    let asset_map: HashMap<i32, &Asset> = assets.iter().map(|asset| (asset.id, asset)).collect();
    let performance_ids: HashSet<i32> = assets
        .iter()
        .filter(|asset| !asset.is_monetary())
        .map(|asset| asset.id)
        .collect();
    let mut replays = Vec::<(i32, LedgerReplay)>::new();
    for (asset_id, transactions) in &transactions_by_asset {
        if !performance_ids.contains(asset_id) {
            continue;
        }
        let replay = match checkpoint.as_ref() {
            Some(_) => ledger::replay_transactions_from_state(
                *asset_id,
                transactions,
                holdings.get(asset_id).copied().unwrap_or_default(),
            ),
            None => ledger::replay_transactions(*asset_id, transactions),
        }
        .map_err(|error| anyhow::anyhow!(error))?;
        replays.push((*asset_id, replay));
    }
    for asset_id in holdings.keys().copied() {
        if !replays.iter().any(|(id, _)| *id == asset_id) {
            replays.push((
                asset_id,
                LedgerReplay {
                    transitions: Vec::new(),
                    final_quantity: holdings.get(&asset_id).copied().unwrap_or_default(),
                    remaining_cost: 0.0,
                },
            ));
        }
    }

    let intervals =
        derive_holding_intervals(start_date, end_date, &holdings, &replays, &performance_ids)?;
    let mut fx_currencies = HashSet::new();
    let mut fx_start = None;
    let mut enriched_transactions = Vec::new();
    for (asset_id, replay) in &replays {
        let Some(asset) = asset_map.get(asset_id) else {
            anyhow::bail!("missing asset {asset_id} for NAV ledger replay");
        };
        if !performance_ids.contains(asset_id) {
            continue;
        }
        let has_interval = intervals
            .iter()
            .any(|interval| interval.asset_id == *asset_id);
        let has_cash_flow = replay.transitions.iter().any(|transition| {
            transition.entry.date >= format_date(start_date)
                && transition.entry.date <= format_date(end_date)
                && !matches!(&transition.effect, LedgerEffect::Split { .. })
        });
        if asset.currency != crate::constants::BASE_CURRENCY && (has_interval || has_cash_flow) {
            fx_currencies.insert(asset.currency.clone());
        }
        let transaction_dates = replay
            .transitions
            .iter()
            .filter(|transition| transition.entry.date >= format_date(start_date))
            .map(|transition| {
                NaiveDate::parse_from_str(&transition.entry.date, crate::constants::DATE_FORMAT)
                    .context("invalid transaction date")
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        let transaction_start = transaction_dates.into_iter().min();
        fx_start = match (fx_start, transaction_start) {
            (None, next) => next,
            (Some(current), Some(next)) => Some(current.min(next)),
            (current, None) => current,
        };
    }
    for interval in &intervals {
        fx_start = Some(fx_start.map_or(interval.start, |date| date.min(interval.start)));
    }
    let mut fx_currencies: Vec<String> = fx_currencies.into_iter().collect();
    fx_currencies.sort();
    let valuation_data = market_data
        .prepare_nav_valuation_data(db, &assets, &intervals, &fx_currencies, fx_start, end_date)
        .await?;

    for (asset_id, replay) in replays {
        let Some(asset) = asset_map.get(&asset_id) else {
            continue;
        };
        if !performance_ids.contains(&asset_id) {
            continue;
        }
        // Enrichment is deliberately performed after MarketData preparation so
        // the same in-memory rates validate transaction-date effects and NAV.
        let rates = valuation_data
            .exchange_rates_for_currency(&asset.currency)
            .cloned()
            .unwrap_or_default();
        let enriched = ledger::enrich_replay(
            &replay,
            &asset.currency,
            crate::constants::BASE_CURRENCY,
            &rates,
        )
        .map_err(|error| anyhow::anyhow!(error))?;
        enriched_transactions.extend(enriched.transitions.into_iter().filter(|transition| {
            transition.transition.entry.date >= format_date(start_date)
                && transition.transition.entry.date <= format_date(end_date)
        }));
    }
    enriched_transactions.sort_by(|left, right| {
        left.transition
            .entry
            .date
            .cmp(&right.transition.entry.date)
            .then(left.transition.entry.id.cmp(&right.transition.entry.id))
    });

    let (effective_end, limitations) = find_calculable_prefix(
        start_date,
        end_date,
        &holdings,
        &enriched_transactions,
        &assets,
        &valuation_data,
    )?;

    Ok(NavRebuildPlan {
        start_date,
        effective_end,
        checkpoint,
        holdings,
        transactions: enriched_transactions,
        assets,
        valuation_data,
        intervals,
        limitations,
    })
}

fn derive_holding_intervals(
    start_date: NaiveDate,
    end_date: NaiveDate,
    holdings: &HashMap<i32, f64>,
    replays: &[(i32, LedgerReplay)],
    performance_ids: &HashSet<i32>,
) -> anyhow::Result<Vec<NavValuationInterval>> {
    let mut intervals = Vec::new();
    for (asset_id, replay) in replays {
        if !performance_ids.contains(asset_id) {
            continue;
        }
        let mut open = (holdings.get(asset_id).copied().unwrap_or_default() > FLOAT_EPSILON)
            .then_some(start_date);
        for transition in &replay.transitions {
            let date =
                NaiveDate::parse_from_str(&transition.entry.date, crate::constants::DATE_FORMAT)
                    .context("invalid transaction date")?;
            if date < start_date || date > end_date {
                continue;
            }
            if transition.quantity_before > FLOAT_EPSILON
                && transition.quantity_after <= FLOAT_EPSILON
            {
                if let Some(interval_start) = open.take() {
                    let interval_end = date - Duration::days(1);
                    if interval_start <= interval_end {
                        intervals.push(NavValuationInterval {
                            asset_id: *asset_id,
                            start: interval_start,
                            end: interval_end,
                        });
                    }
                }
            } else if transition.quantity_before <= FLOAT_EPSILON
                && transition.quantity_after > FLOAT_EPSILON
            {
                open = Some(date);
            }
        }
        if let Some(interval_start) = open {
            intervals.push(NavValuationInterval {
                asset_id: *asset_id,
                start: interval_start,
                end: end_date,
            });
        }
    }
    intervals.sort_by_key(|interval| (interval.start, interval.asset_id));
    Ok(intervals)
}

fn find_calculable_prefix(
    start_date: NaiveDate,
    end_date: NaiveDate,
    checkpoint_holdings: &HashMap<i32, f64>,
    transactions: &[EnrichedLedgerTransition],
    assets: &[Asset],
    valuation_data: &NavValuationData,
) -> anyhow::Result<(NaiveDate, Vec<MarketDataLimitation>)> {
    let asset_map: HashMap<i32, &Asset> = assets.iter().map(|asset| (asset.id, asset)).collect();
    let mut holdings = checkpoint_holdings.clone();
    let mut by_date = HashMap::<String, Vec<&EnrichedLedgerTransition>>::new();
    for transaction in transactions {
        by_date
            .entry(transaction.transition.entry.date.clone())
            .or_default()
            .push(transaction);
    }
    let mut limitations = Vec::new();
    let mut transaction_limitations = Vec::new();
    let mut first_blocked_date = None;
    let mut current = start_date;
    while current <= end_date {
        let date = format_date(current);
        let mut blocked = false;
        if let Some(day_transactions) = by_date.get(&date) {
            for transaction in day_transactions {
                let asset = asset_map
                    .get(&transaction.transition.entry.asset_id)
                    .context("missing asset for NAV transaction")?;
                if asset.is_monetary() {
                    continue;
                }
                let missing_conversion = match transaction.transition.effect {
                    LedgerEffect::Buy { .. } => transaction.buy_contribution.is_none(),
                    LedgerEffect::Sell { .. } => transaction.sell_withdrawal.is_none(),
                    LedgerEffect::Dividend { .. } => transaction.dividend_income.is_none(),
                    LedgerEffect::Split { .. } => false,
                };
                if missing_conversion {
                    if let Some(limitation) =
                        conversion_limitation(asset, valuation_data, current, end_date)
                    {
                        transaction_limitations.push(limitation);
                    }
                    blocked = true;
                    first_blocked_date.get_or_insert(current);
                }
                holdings.insert(
                    transaction.transition.entry.asset_id,
                    transaction.transition.quantity_after,
                );
            }
        }
        for (asset_id, quantity) in &holdings {
            if *quantity <= FLOAT_EPSILON {
                continue;
            }
            let asset = asset_map
                .get(asset_id)
                .context("missing asset for NAV valuation")?;
            if asset.is_monetary() {
                continue;
            }
            if !valuation_data.has_price_on(*asset_id, current) {
                if let Some(limitation) = valuation_data.price_limitation(asset, current, end_date)
                {
                    add_limitation(&mut limitations, limitation);
                }
                blocked = true;
            }
            if !valuation_data.has_fx_on(asset, current) {
                if let Some(limitation) =
                    valuation_data.fx_limitation(&asset.currency, current, end_date)
                {
                    add_limitation(&mut limitations, limitation);
                }
                blocked = true;
            }
        }
        for limitation in transaction_limitations.drain(..) {
            add_limitation(&mut limitations, limitation);
        }
        if blocked {
            first_blocked_date.get_or_insert(current);
        }
        current += Duration::days(1);
    }
    Ok((
        first_blocked_date.map_or(end_date, |date| date - Duration::days(1)),
        limitations,
    ))
}

fn conversion_limitation(
    asset: &Asset,
    valuation_data: &NavValuationData,
    date: NaiveDate,
    end_date: NaiveDate,
) -> Option<MarketDataLimitation> {
    if asset.currency == crate::constants::BASE_CURRENCY {
        None
    } else {
        valuation_data.fx_limitation(&asset.currency, date, end_date)
    }
}

fn add_limitation(limitations: &mut Vec<MarketDataLimitation>, limitation: MarketDataLimitation) {
    if !limitations.contains(&limitation) {
        limitations.push(limitation);
    }
}

async fn execute_rebuild_plan(sink: &NavSnapshotSink, plan: &NavRebuildPlan) -> anyhow::Result<()> {
    tracing::debug!(
        interval_count = plan.intervals.len(),
        "executing prepared NAV valuation intervals"
    );
    tracing::info!(%plan.start_date, %plan.effective_end, "rebuilding portfolio history from prepared plan");
    if plan.effective_end < plan.start_date {
        return Ok(());
    }

    let asset_map: HashMap<i32, &Asset> =
        plan.assets.iter().map(|asset| (asset.id, asset)).collect();
    let mut holdings = plan.holdings.clone();
    let mut outstanding_shares = plan
        .checkpoint
        .as_ref()
        .map_or(0.0, |snapshot| snapshot.outstanding_shares);
    let mut nav = plan
        .checkpoint
        .as_ref()
        .map_or(INITIAL_NAV, |snapshot| snapshot.nav);
    let mut accumulated_cash = plan
        .checkpoint
        .as_ref()
        .map_or(0.0, |snapshot| snapshot.total_value - snapshot.asset_value);
    let mut fresh = plan.checkpoint.is_none();
    let mut by_date = HashMap::<String, Vec<&EnrichedLedgerTransition>>::new();
    for transaction in &plan.transactions {
        by_date
            .entry(transaction.transition.entry.date.clone())
            .or_default()
            .push(transaction);
    }
    let mut batch = SnapshotBatch {
        snapshots: Vec::new(),
        generated_rows: 0,
    };
    let mut current = plan.start_date;
    while current <= plan.effective_end {
        let date = format_date(current);
        let day_transactions = by_date.get(&date).map_or(&[][..], Vec::as_slice);
        let has_performance_transactions = day_transactions.iter().any(|transaction| {
            asset_map
                .get(&transaction.transition.entry.asset_id)
                .is_some_and(|asset| !asset.is_monetary())
        });
        let (new_shares, new_nav, dividend_income) = process_day_transactions(
            day_transactions,
            &mut holdings,
            outstanding_shares,
            nav,
            &asset_map,
        )?;
        outstanding_shares = new_shares;
        nav = new_nav;
        accumulated_cash += dividend_income;

        if outstanding_shares == 0.0 && fresh && !has_performance_transactions {
            current += Duration::days(1);
            continue;
        }
        let (asset_value, asset_values) =
            compute_day_asset_values(&plan.valuation_data, &holdings, &asset_map, &date, current)?;
        let total_value = asset_value + accumulated_cash;
        if outstanding_shares > 0.0 {
            nav = total_value / outstanding_shares;
        }
        validate_nav_state(&date, outstanding_shares, nav, asset_value, total_value)?;
        if fresh && has_performance_transactions {
            batch.push(
                PortfolioSnapshot {
                    date: format_date(current - Duration::days(1)),
                    asset_value: 0.0,
                    total_value: 0.0,
                    outstanding_shares: 0.0,
                    nav: INITIAL_NAV,
                },
                Vec::new(),
            );
            fresh = false;
        }
        if batch.would_exceed_targets(asset_values.len()) {
            persist_snapshot_batch(sink, std::mem::take(&mut batch)).await?;
        }
        batch.push(
            PortfolioSnapshot {
                date,
                asset_value,
                total_value,
                outstanding_shares,
                nav,
            },
            asset_values,
        );
        current += Duration::days(1);
    }
    persist_snapshot_batch(sink, batch).await
}

fn process_day_transactions(
    day_transactions: &[&EnrichedLedgerTransition],
    holdings: &mut HashMap<i32, f64>,
    outstanding_shares: f64,
    nav: f64,
    asset_map: &HashMap<i32, &Asset>,
) -> anyhow::Result<(f64, f64, f64)> {
    let mut shares = outstanding_shares;
    let mut current_nav = nav;
    let mut dividend_income = 0.0;
    for transaction in day_transactions {
        let entry = &transaction.transition.entry;
        if asset_map
            .get(&entry.asset_id)
            .is_some_and(|asset| asset.is_monetary())
        {
            continue;
        }
        match &transaction.transition.effect {
            LedgerEffect::Split { .. } => {}
            LedgerEffect::Dividend { .. } => {
                dividend_income += transaction
                    .dividend_income
                    .context("prepared NAV plan is missing dividend conversion")?;
            }
            LedgerEffect::Sell { .. } => {
                let withdrawal = transaction
                    .sell_withdrawal
                    .context("prepared NAV plan is missing sell conversion")?;
                if shares > 0.0 && current_nav > 0.0 {
                    shares -= withdrawal / current_nav;
                    if shares < 0.0 {
                        shares = 0.0;
                    }
                } else if shares > 0.0 {
                    anyhow::bail!(
                        "cannot process NAV sell entry {} with non-positive NAV",
                        entry.id
                    );
                }
            }
            LedgerEffect::Buy { .. } => {
                let contribution = transaction
                    .buy_contribution
                    .context("prepared NAV plan is missing buy conversion")?;
                if shares == 0.0 {
                    current_nav = INITIAL_NAV;
                    shares = contribution / INITIAL_NAV;
                } else {
                    anyhow::ensure!(
                        current_nav.is_finite() && current_nav > 0.0,
                        "cannot process NAV contribution for entry {} on {} with non-positive or non-finite NAV {}",
                        entry.id,
                        entry.date,
                        current_nav
                    );
                    shares += contribution / current_nav;
                }
            }
        }
        holdings.insert(entry.asset_id, transaction.transition.quantity_after);
    }
    anyhow::ensure!(
        shares.is_finite() && current_nav.is_finite() && dividend_income.is_finite(),
        "non-finite NAV transaction state"
    );
    Ok((shares, current_nav, dividend_income))
}

fn validate_nav_state(
    date: &str,
    shares: f64,
    nav: f64,
    asset_value: f64,
    total_value: f64,
) -> anyhow::Result<()> {
    anyhow::ensure!(shares.is_finite() && shares >= 0.0 && nav.is_finite() && asset_value.is_finite() && asset_value >= 0.0 && total_value.is_finite() && total_value >= 0.0,
        "invalid NAV state on {date}: shares={shares}, nav={nav}, asset_value={asset_value}, total_value={total_value}");
    Ok(())
}

fn compute_day_asset_values(
    valuation_data: &NavValuationData,
    holdings: &HashMap<i32, f64>,
    asset_map: &HashMap<i32, &Asset>,
    date: &str,
    as_of: NaiveDate,
) -> anyhow::Result<(f64, Vec<AssetSnapshot>)> {
    let mut total = 0.0;
    let mut snapshots = Vec::new();
    for (&asset_id, &quantity) in holdings {
        if quantity <= FLOAT_EPSILON {
            continue;
        }
        let asset = asset_map
            .get(&asset_id)
            .context("missing asset for NAV valuation")?;
        if asset.is_monetary() {
            continue;
        }
        let valuation = valuation_data.valuation(asset, as_of)?;
        let market_value = quantity * valuation.base_currency_price;
        total += market_value;
        snapshots.push(AssetSnapshot {
            date: date.to_owned(),
            asset_id,
            quantity,
            closing_price: valuation.native_price,
            market_value,
            exchange_rate: valuation.fx_rate,
        });
    }
    Ok((total, snapshots))
}

async fn persist_snapshot_batch(
    sink: &NavSnapshotSink,
    batch: SnapshotBatch,
) -> anyhow::Result<()> {
    if batch.snapshots.is_empty() {
        return Ok(());
    }
    for snapshot in &batch.snapshots {
        validate_nav_state(
            &snapshot.portfolio.date,
            snapshot.portfolio.outstanding_shares,
            snapshot.portfolio.nav,
            snapshot.portfolio.asset_value,
            snapshot.portfolio.total_value,
        )?;
        for asset in &snapshot.assets {
            anyhow::ensure!(
                asset.quantity.is_finite()
                    && asset.quantity >= 0.0
                    && asset.closing_price.is_finite()
                    && asset.market_value.is_finite()
                    && asset.market_value >= 0.0
                    && asset.exchange_rate.is_finite(),
                "invalid asset NAV state on {} for asset {}",
                asset.date,
                asset.asset_id
            );
        }
    }
    let SnapshotBatch {
        snapshots,
        generated_rows,
    } = batch;
    let mut portfolio_snapshots = Vec::with_capacity(snapshots.len());
    let mut asset_snapshots = Vec::with_capacity(generated_rows.saturating_sub(snapshots.len()));
    for CompleteNavSnapshot { portfolio, assets } in snapshots {
        portfolio_snapshots.push(portfolio);
        asset_snapshots.extend(assets);
    }
    sink.persist(&portfolio_snapshots, &asset_snapshots).await
}

impl SnapshotBatch {
    fn would_exceed_targets(&self, asset_count: usize) -> bool {
        !self.snapshots.is_empty()
            && (self.snapshots.len() >= SNAPSHOT_BATCH_DATE_TARGET
                || self.generated_rows + 1 + asset_count > SNAPSHOT_BATCH_ROW_TARGET)
    }

    fn push(&mut self, portfolio: PortfolioSnapshot, assets: Vec<AssetSnapshot>) {
        self.generated_rows += 1 + assets.len();
        self.snapshots
            .push(CompleteNavSnapshot { portfolio, assets });
    }
}
