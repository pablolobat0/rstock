use std::collections::HashMap;

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate};
use futures::stream::{self, StreamExt};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, is_benchmark_ticker, BASE_CURRENCY, DATE_FORMAT, FIVE_YEAR_DAYS, FLOAT_EPSILON,
    ONE_YEAR_DAYS, THREE_YEAR_DAYS,
};
use crate::db::repos::{asset_repo, portfolio_history_repo, transaction_repo};
use crate::models::{
    Asset, CurrentPosition, CurrentPositions, MarketDataLimitation,
    MarketDataLimitationClassification, MarketDataSubject, PortfolioResult, Transaction,
};
use crate::services::market_data::MarketData;
use crate::services::{analytics, metrics};
use crate::services::{ledger, nav};

const CURRENT_POSITION_CONCURRENCY_LIMIT: usize = 4;

#[derive(Clone, Copy)]
enum LedgerPreparationScope {
    AllOpenHoldings,
    ReuseNavPerformanceData,
}

#[allow(clippy::too_many_lines)]
pub async fn get_portfolio(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<PortfolioResult> {
    let today = market_data.today();
    // NAV readiness is strict: missing historical market data is represented as
    // a NAV-limited readiness with nullable NAV facts, while genuine failures
    // (DB, parsing, invariants) propagate.
    let nav_readiness = nav::ensure_portfolio_history(db, market_data).await?;
    let current_positions = get_current_positions_inner(
        db,
        market_data,
        if nav_readiness.performance_market_data_prepared {
            LedgerPreparationScope::ReuseNavPerformanceData
        } else {
            LedgerPreparationScope::AllOpenHoldings
        },
    )
    .await?;
    let mut nav_market_data_limitations = nav_readiness.market_data_limitations;

    let Some(current_snapshot) = &nav_readiness.latest_snapshot else {
        return Ok(result_without_nav(
            current_positions,
            nav_market_data_limitations,
        ));
    };

    let snapshot_date = current_snapshot.date.clone();
    let current_nav = current_snapshot.nav;
    let total_value = current_snapshot.total_value;

    let snap_date =
        NaiveDate::parse_from_str(&snapshot_date, DATE_FORMAT).context("invalid snapshot date")?;

    let (daily_change, daily_change_pct) = compute_daily_change(db, snap_date, total_value).await?;
    let inception_date = portfolio_history_repo::find_earliest(db)
        .await?
        .map(|s| s.date);

    let (ytd_date, one_year_date, three_year_date, five_year_date) =
        compute_period_returns_dates(today);

    let ytd_return = calc_return(db, &snapshot_date, current_nav, &ytd_date, true, false).await?;
    let one_year_return = calc_return(
        db,
        &snapshot_date,
        current_nav,
        &one_year_date,
        false,
        false,
    )
    .await?;
    let three_year_return = calc_return(
        db,
        &snapshot_date,
        current_nav,
        &three_year_date,
        false,
        true,
    )
    .await?;
    let five_year_return = calc_return(
        db,
        &snapshot_date,
        current_nav,
        &five_year_date,
        false,
        true,
    )
    .await?;

    let period_metrics = analytics::compute_all_period_metrics(
        db,
        &snapshot_date,
        &ytd_date,
        &one_year_date,
        &three_year_date,
        &five_year_date,
        market_data,
    )
    .await?;

    extend_unique_limitations(
        &mut nav_market_data_limitations,
        period_metrics.market_data_limitations.clone(),
    );

    Ok(PortfolioResult {
        base_currency: BASE_CURRENCY.to_string(),
        rows: current_positions.positions,
        monetary_positions: current_positions.monetary_positions,
        total_current_value: current_positions.total_current_value,
        total_monetary_value: current_positions.total_monetary_value,
        total_value: current_positions.total_value,
        total_invested: current_positions.total_invested,
        total_monetary_invested: current_positions.total_monetary_invested,
        total_dividends: current_positions.total_dividends,
        total_monetary_dividends: current_positions.total_monetary_dividends,
        total_open_position_gain_loss: current_positions.total_open_position_gain_loss,
        total_open_position_gain_loss_pct: current_positions.total_open_position_gain_loss_pct,
        total_monetary_open_position_gain_loss: current_positions
            .total_monetary_open_position_gain_loss,
        total_monetary_open_position_gain_loss_pct: current_positions
            .total_monetary_open_position_gain_loss_pct,
        snapshot_date: Some(snapshot_date),
        nav: Some(current_nav),
        daily_change,
        daily_change_pct,
        inception_date,
        ytd_return,
        one_year_return,
        three_year_return,
        five_year_return,
        ytd_metrics: period_metrics.ytd,
        one_year_metrics: period_metrics.one_year,
        three_year_metrics: period_metrics.three_year,
        five_year_metrics: period_metrics.five_year,
        nav_market_data_limitations,
        current_position_market_data_limitations: current_positions.market_data_limitations,
        monetary_market_data_limitations: current_positions.monetary_market_data_limitations,
    })
}

#[allow(clippy::too_many_lines)]
pub async fn get_current_positions(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<CurrentPositions> {
    get_current_positions_inner(db, market_data, LedgerPreparationScope::AllOpenHoldings).await
}

#[allow(clippy::too_many_lines)]
async fn get_current_positions_inner(
    db: &DatabaseConnection,
    market_data: &MarketData,
    preparation_scope: LedgerPreparationScope,
) -> anyhow::Result<CurrentPositions> {
    let current_date = market_data.today();
    let today = format_date(current_date);
    let transactions = transaction_repo::find_all_ordered_by_date(db, None, Some(&today)).await?;
    if transactions.is_empty() {
        return Ok(empty_current_positions());
    }

    let mut transactions_by_asset: HashMap<i32, Vec<Transaction>> = HashMap::new();
    for transaction in transactions {
        transactions_by_asset
            .entry(transaction.asset_id)
            .or_default()
            .push(transaction);
    }
    let assets = asset_repo::find_by_ids(db, transactions_by_asset.keys().copied()).await?;
    let end_date = format_date(current_date - Duration::days(1));

    // Prepare historical prices and FX before projecting ledger facts so that
    // transaction-date FX is cached (fetch/persist, then read) and cost and
    // dividend facts are identical across repeated requests.
    let Some(prepare_scope) = ledger_prepare_scope(
        &assets,
        &transactions_by_asset,
        &end_date,
        preparation_scope,
    )?
    else {
        return Ok(empty_current_positions());
    };
    if !prepare_scope.assets.is_empty() {
        market_data
            .prepare_individual_price_market_data(
                db,
                &prepare_scope.assets,
                &prepare_scope.start_date,
                &end_date,
            )
            .await?;
    }

    let projections = project_open_holdings(db, market_data, assets, transactions_by_asset).await?;
    if projections.is_empty() {
        return Ok(empty_current_positions());
    }

    let mut positions = Vec::new();
    let mut monetary_positions = Vec::new();
    let mut market_data_limitations = Vec::new();
    let mut monetary_market_data_limitations = Vec::new();
    let projected_positions = stream::iter(projections)
        .map(|projection| async move {
            let is_monetary = projection.asset.is_monetary();
            let position = current_position_from_projection(db, market_data, projection).await?;
            Ok::<_, anyhow::Error>((is_monetary, position))
        })
        .buffered(CURRENT_POSITION_CONCURRENCY_LIMIT)
        .collect::<Vec<_>>()
        .await;
    for projected_position in projected_positions {
        let (is_monetary, position) = projected_position?;
        if is_monetary {
            extend_unique_limitations(
                &mut monetary_market_data_limitations,
                position.market_data_limitations.clone(),
            );
            monetary_positions.push(position);
        } else {
            extend_unique_limitations(
                &mut market_data_limitations,
                position.market_data_limitations.clone(),
            );
            positions.push(position);
        }
    }

    let position_totals = aggregate_position_totals(&positions);
    let monetary_totals = aggregate_position_totals(&monetary_positions);
    let total_value = position_totals
        .current_value
        .zip(monetary_totals.current_value)
        .map(|(a, b)| a + b);

    Ok(CurrentPositions {
        base_currency: BASE_CURRENCY.to_owned(),
        positions,
        monetary_positions,
        total_current_value: position_totals.current_value,
        total_monetary_value: monetary_totals.current_value,
        total_value,
        total_invested: position_totals.invested,
        total_monetary_invested: monetary_totals.invested,
        total_dividends: position_totals.dividends,
        total_monetary_dividends: monetary_totals.dividends,
        total_open_position_gain_loss: position_totals.open_position_gain_loss,
        total_open_position_gain_loss_pct: position_totals.open_position_gain_loss_pct,
        total_monetary_open_position_gain_loss: monetary_totals.open_position_gain_loss,
        total_monetary_open_position_gain_loss_pct: monetary_totals.open_position_gain_loss_pct,
        market_data_limitations,
        monetary_market_data_limitations,
    })
}

// --- Private helpers ---

struct HoldingProjection {
    asset: Asset,
    total_qty: f64,
    total_invested: Option<f64>,
    dividends_received: Option<f64>,
    market_data_limitations: Vec<MarketDataLimitation>,
}

struct PositionTotals {
    current_value: Option<f64>,
    invested: Option<f64>,
    dividends: Option<f64>,
    open_position_gain_loss: Option<f64>,
    open_position_gain_loss_pct: Option<f64>,
}

/// The subset of current ledger assets and the price/FX range that must be
/// prepared before any ledger fact that depends on historical FX is projected.
struct LedgerPrepareScope {
    assets: Vec<Asset>,
    start_date: String,
}

/// Derives the open-holding asset set and earliest transaction date from the
/// ledger alone (no market data reads) so that historical prices and FX can be
/// prepared before currency-dependent cost and dividend facts are projected.
fn ledger_prepare_scope(
    assets: &[Asset],
    transactions_by_asset: &HashMap<i32, Vec<Transaction>>,
    end_date: &str,
    preparation_scope: LedgerPreparationScope,
) -> anyhow::Result<Option<LedgerPrepareScope>> {
    let mut prepare_assets = Vec::new();
    let mut earliest_transaction_date: Option<NaiveDate> = None;
    let mut has_open_holding = false;
    for asset in assets {
        if is_benchmark_ticker(&asset.ticker) {
            continue;
        }
        let Some(transactions) = transactions_by_asset.get(&asset.id) else {
            continue;
        };
        let replay = ledger::replay_transactions(asset.id, transactions)
            .map_err(|error| anyhow::anyhow!(error))?;
        if replay.final_quantity > FLOAT_EPSILON {
            let needs_preparation = match preparation_scope {
                LedgerPreparationScope::AllOpenHoldings => true,
                LedgerPreparationScope::ReuseNavPerformanceData => {
                    asset.is_monetary() || transactions.iter().any(|tx| tx.date.as_str() > end_date)
                }
            };
            if !needs_preparation {
                continue;
            }
            has_open_holding = true;
            prepare_assets.push(asset.clone());
            let earliest = NaiveDate::parse_from_str(
                replay
                    .transitions
                    .first()
                    .map_or(end_date, |transition| transition.entry.date.as_str()),
                DATE_FORMAT,
            )?;
            earliest_transaction_date = Some(match earliest_transaction_date {
                Some(previous) => previous.min(earliest),
                None => earliest,
            });
        }
    }
    if !has_open_holding {
        return Ok(None);
    }
    let start_date = match earliest_transaction_date {
        Some(date) => format_date(date).min(end_date.to_owned()),
        None => end_date.to_owned(),
    };
    Ok(Some(LedgerPrepareScope {
        assets: prepare_assets,
        start_date,
    }))
}

fn empty_current_positions() -> CurrentPositions {
    CurrentPositions {
        base_currency: BASE_CURRENCY.to_owned(),
        positions: Vec::new(),
        monetary_positions: Vec::new(),
        total_current_value: Some(0.0),
        total_monetary_value: Some(0.0),
        total_value: Some(0.0),
        total_invested: Some(0.0),
        total_monetary_invested: Some(0.0),
        total_dividends: Some(0.0),
        total_monetary_dividends: Some(0.0),
        total_open_position_gain_loss: Some(0.0),
        total_open_position_gain_loss_pct: Some(0.0),
        total_monetary_open_position_gain_loss: Some(0.0),
        total_monetary_open_position_gain_loss_pct: Some(0.0),
        market_data_limitations: Vec::new(),
        monetary_market_data_limitations: Vec::new(),
    }
}

async fn project_open_holdings(
    db: &DatabaseConnection,
    market_data: &MarketData,
    assets: Vec<Asset>,
    transactions_by_asset: HashMap<i32, Vec<Transaction>>,
) -> anyhow::Result<Vec<HoldingProjection>> {
    let mut projections = Vec::new();
    for asset in assets {
        if is_benchmark_ticker(&asset.ticker) {
            continue;
        }
        let transactions = transactions_by_asset
            .get(&asset.id)
            .context("asset has no ordered transactions")?;
        let projection = project_holding(db, market_data, asset, transactions).await?;
        if projection.total_qty > FLOAT_EPSILON {
            projections.push(projection);
        }
    }
    Ok(projections)
}

async fn project_holding(
    db: &DatabaseConnection,
    market_data: &MarketData,
    asset: Asset,
    transactions: &[Transaction],
) -> anyhow::Result<HoldingProjection> {
    let replay = ledger::replay_transactions(asset.id, transactions)
        .map_err(|error| anyhow::anyhow!(error))?;
    let first_date = replay
        .transitions
        .first()
        .map(|transition| transition.entry.date.as_str())
        .context("open holding has no transactions")?;
    let rates = market_data
        .get_asset_exchange_rates(db, &asset, first_date, &format_date(market_data.today()))
        .await?;
    let enriched = ledger::enrich_replay(&replay, &asset.currency, BASE_CURRENCY, &rates)
        .map_err(|error| anyhow::anyhow!(error))?;
    let mut market_data_limitations = Vec::new();
    if asset.currency != BASE_CURRENCY {
        for transition in &enriched.transitions {
            if (transition.buy_contribution.is_none()
                && transition.entry_type() == ledger::LedgerEntryType::Buy)
                || (transition.dividend_income.is_none()
                    && transition.entry_type() == ledger::LedgerEntryType::Dividend)
            {
                let date =
                    NaiveDate::parse_from_str(&transition.transition.entry.date, DATE_FORMAT)
                        .context("invalid transaction date")?;
                extend_unique_limitations(
                    &mut market_data_limitations,
                    vec![MarketDataLimitation {
                        subject: MarketDataSubject::FxRate {
                            currency: asset.currency.clone(),
                        },
                        latest_available_date: None,
                        requested_end_date: date,
                        classification: MarketDataLimitationClassification::ActionableMissingData,
                    }],
                );
            }
        }
    }

    Ok(HoldingProjection {
        asset,
        total_qty: enriched.final_quantity,
        total_invested: enriched.remaining_cost,
        dividends_received: enriched.dividends,
        market_data_limitations,
    })
}

async fn current_position_from_projection(
    db: &DatabaseConnection,
    market_data: &MarketData,
    projection: HoldingProjection,
) -> anyhow::Result<CurrentPosition> {
    let individual_price = market_data
        .individual_price_if_available(db, &projection.asset)
        .await?;
    let current_value = individual_price
        .native_price
        .zip(individual_price.fx_rate)
        .map(|(price, rate)| projection.total_qty * price * rate);
    let avg_cost = projection
        .total_invested
        .map(|cost| cost / projection.total_qty);
    let open_position_gain_loss = current_value
        .zip(projection.total_invested)
        .map(|(value, cost)| value - cost);
    let open_position_gain_loss_pct =
        open_position_gain_loss
            .zip(projection.total_invested)
            .map(|(gain_loss, cost)| {
                if cost.abs() < FLOAT_EPSILON {
                    0.0
                } else {
                    (gain_loss / cost) * 100.0
                }
            });

    Ok(CurrentPosition {
        ticker: projection.asset.ticker,
        name: projection.asset.name,
        asset_type: projection.asset.asset_type,
        currency: projection.asset.currency,
        morningstar_code: projection.asset.morningstar_code,
        asset_class: projection.asset.asset_class,
        equity_style: projection.asset.equity_style,
        management: projection.asset.management,
        total_qty: projection.total_qty,
        avg_cost,
        current_price: individual_price.native_price,
        price_date: individual_price.price_date,
        total_invested: projection.total_invested,
        current_value,
        dividends_received: projection.dividends_received,
        open_position_gain_loss,
        open_position_gain_loss_pct,
        market_data_limitations: {
            let mut limitations = projection.market_data_limitations;
            extend_unique_limitations(&mut limitations, individual_price.limitations);
            limitations
        },
    })
}

fn aggregate_position_totals(positions: &[CurrentPosition]) -> PositionTotals {
    let current_value = complete_sum(positions.iter().map(|position| position.current_value));
    let invested = complete_sum(positions.iter().map(|position| position.total_invested));
    let dividends = complete_sum(positions.iter().map(|position| position.dividends_received));
    let open_position_gain_loss = complete_sum(
        positions
            .iter()
            .map(|position| position.open_position_gain_loss),
    );
    let open_position_gain_loss_pct =
        open_position_gain_loss
            .zip(invested)
            .map(|(gain_loss, invested)| {
                if invested.abs() < FLOAT_EPSILON {
                    0.0
                } else {
                    (gain_loss / invested) * 100.0
                }
            });
    PositionTotals {
        current_value,
        invested,
        dividends,
        open_position_gain_loss,
        open_position_gain_loss_pct,
    }
}

fn complete_sum(mut values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    values.try_fold(0.0, |sum, value| value.map(|value| sum + value))
}

fn result_without_nav(
    current_positions: CurrentPositions,
    nav_market_data_limitations: Vec<MarketDataLimitation>,
) -> PortfolioResult {
    PortfolioResult {
        base_currency: BASE_CURRENCY.to_string(),
        rows: current_positions.positions,
        monetary_positions: current_positions.monetary_positions,
        total_current_value: current_positions.total_current_value,
        total_monetary_value: current_positions.total_monetary_value,
        total_value: current_positions.total_value,
        total_invested: current_positions.total_invested,
        total_monetary_invested: current_positions.total_monetary_invested,
        total_dividends: current_positions.total_dividends,
        total_monetary_dividends: current_positions.total_monetary_dividends,
        total_open_position_gain_loss: current_positions.total_open_position_gain_loss,
        total_open_position_gain_loss_pct: current_positions.total_open_position_gain_loss_pct,
        total_monetary_open_position_gain_loss: current_positions
            .total_monetary_open_position_gain_loss,
        total_monetary_open_position_gain_loss_pct: current_positions
            .total_monetary_open_position_gain_loss_pct,
        snapshot_date: None,
        nav: None,
        daily_change: None,
        daily_change_pct: None,
        inception_date: None,
        ytd_return: None,
        one_year_return: None,
        three_year_return: None,
        five_year_return: None,
        ytd_metrics: None,
        one_year_metrics: None,
        three_year_metrics: None,
        five_year_metrics: None,
        nav_market_data_limitations,
        current_position_market_data_limitations: current_positions.market_data_limitations,
        monetary_market_data_limitations: current_positions.monetary_market_data_limitations,
    }
}

fn extend_unique_limitations(
    limitations: &mut Vec<MarketDataLimitation>,
    additional: Vec<MarketDataLimitation>,
) {
    for limitation in additional {
        if !limitations.contains(&limitation) {
            limitations.push(limitation);
        }
    }
}

async fn compute_daily_change(
    db: &DatabaseConnection,
    snap_date: NaiveDate,
    total_value: f64,
) -> anyhow::Result<(Option<f64>, Option<f64>)> {
    let prev_day = format_date(snap_date - chrono::Duration::days(1));
    if let Some(prev) = portfolio_history_repo::find_at_or_before(db, &prev_day).await? {
        if prev.total_value > 0.0 {
            let change = total_value - prev.total_value;
            let change_pct = (change / prev.total_value) * 100.0;
            return Ok((Some(change), Some(change_pct)));
        }
    }
    Ok((None, None))
}

fn compute_period_returns_dates(today: NaiveDate) -> (String, String, String, String) {
    let ytd =
        format_date(NaiveDate::from_ymd_opt(today.year(), 1, 1).expect("Jan 1 is always valid"));
    let one_year = format_date(today - chrono::Duration::days(ONE_YEAR_DAYS));
    let three_year = format_date(today - chrono::Duration::days(THREE_YEAR_DAYS));
    let five_year = format_date(today - chrono::Duration::days(FIVE_YEAR_DAYS));
    (ytd, one_year, three_year, five_year)
}

async fn calc_return(
    db: &DatabaseConnection,
    current_date: &str,
    current_nav: f64,
    target_date: &str,
    fallback_to_inception: bool,
    annualize: bool,
) -> anyhow::Result<Option<f64>> {
    let snapshot = match portfolio_history_repo::find_at_or_before(db, target_date).await? {
        Some(s) => s,
        None if fallback_to_inception => match portfolio_history_repo::find_earliest(db).await? {
            Some(s) => s,
            None => return Ok(None),
        },
        None => return Ok(None),
    };
    if snapshot.nav > 0.0 {
        let ret = if annualize {
            metrics::compute_cagr(&snapshot.date, current_date, snapshot.nav, current_nav)
        } else {
            Some(((current_nav - snapshot.nav) / snapshot.nav) * 100.0)
        };
        Ok(ret)
    } else {
        Ok(None)
    }
}
