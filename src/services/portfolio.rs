use std::collections::HashMap;

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, is_benchmark_ticker, BASE_CURRENCY, DATE_FORMAT, FIVE_YEAR_DAYS, FLOAT_EPSILON,
    ONE_YEAR_DAYS, THREE_YEAR_DAYS,
};
use crate::db::repos::{asset_repo, portfolio_history_repo, transaction_repo};
use crate::models::{
    cents_to_f64, Asset, CurrentPosition, CurrentPositions, MarketDataLimitation,
    MarketDataLimitationClassification, MarketDataSubject, PortfolioResult, PortfolioSnapshot,
    Transaction,
};
use crate::services::market_data::MarketData;
use crate::services::nav;
use crate::services::{analytics, metrics};

#[allow(clippy::too_many_lines)]
pub async fn get_portfolio(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<PortfolioResult> {
    let today = market_data.today();
    nav::ensure_portfolio_history(db, market_data).await?;
    let current_positions = get_current_positions(db, market_data).await?;

    let latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    let Some(current_snapshot) = &latest_snapshot else {
        return Ok(result_without_nav(current_positions));
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

    let mut nav_market_data_limitations = Vec::new();
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
        total_dividends: current_positions.total_dividends,
        total_open_position_gain_loss: current_positions.total_open_position_gain_loss,
        total_open_position_gain_loss_pct: current_positions.total_open_position_gain_loss_pct,
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
#[allow(dead_code)] // Public library interface; CLI migration is a later ticket.
pub async fn get_current_positions(
    db: &DatabaseConnection,
    market_data: &MarketData,
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
    let projections = project_open_holdings(db, market_data, assets, transactions_by_asset).await?;
    if projections.is_empty() {
        return Ok(empty_current_positions());
    }

    let end_date = format_date(current_date - Duration::days(1));
    let earliest_transaction_date = projections
        .iter()
        .map(|projection| projection.earliest_transaction_date.as_str())
        .min()
        .context("open holdings have no transactions")?;
    let start_date = earliest_transaction_date.min(end_date.as_str());
    let assets_to_prepare: Vec<Asset> = projections
        .iter()
        .map(|projection| projection.asset.clone())
        .collect();
    market_data
        .prepare_individual_price_market_data(db, &assets_to_prepare, start_date, &end_date)
        .await?;

    let mut positions = Vec::new();
    let mut monetary_positions = Vec::new();
    let mut market_data_limitations = Vec::new();
    let mut monetary_market_data_limitations = Vec::new();
    for projection in projections {
        let is_monetary = projection.asset.is_monetary();
        let position = current_position_from_projection(db, market_data, projection).await?;
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

    let total_current_value = complete_sum(positions.iter().map(|position| position.current_value));
    let total_monetary_value = complete_sum(
        monetary_positions
            .iter()
            .map(|position| position.current_value),
    );
    let total_value = total_current_value
        .zip(total_monetary_value)
        .map(|(a, b)| a + b);
    let total_invested = complete_sum(positions.iter().map(|position| position.total_invested));
    let total_dividends =
        complete_sum(positions.iter().map(|position| position.dividends_received));
    let total_open_position_gain_loss = complete_sum(
        positions
            .iter()
            .map(|position| position.open_position_gain_loss),
    );
    let total_open_position_gain_loss_pct =
        total_open_position_gain_loss
            .zip(total_invested)
            .map(|(gain_loss, invested)| {
                if invested.abs() < FLOAT_EPSILON {
                    0.0
                } else {
                    (gain_loss / invested) * 100.0
                }
            });

    Ok(CurrentPositions {
        base_currency: BASE_CURRENCY.to_owned(),
        positions,
        monetary_positions,
        total_current_value,
        total_monetary_value,
        total_value,
        total_invested,
        total_dividends,
        total_open_position_gain_loss,
        total_open_position_gain_loss_pct,
        market_data_limitations,
        monetary_market_data_limitations,
    })
}

pub async fn get_nav_snapshots(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
    market_data: &MarketData,
) -> anyhow::Result<Vec<PortfolioSnapshot>> {
    nav::ensure_portfolio_history(db, market_data).await?;
    portfolio_history_repo::find_between(db, start_date, end_date).await
}

pub async fn get_inception_date(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<Option<String>> {
    nav::ensure_portfolio_history(db, market_data).await?;
    let earliest = portfolio_history_repo::find_earliest(db).await?;
    Ok(earliest.map(|s| s.date))
}

// --- Private helpers ---

#[allow(dead_code)]
struct HoldingProjection {
    asset: Asset,
    total_qty: f64,
    earliest_transaction_date: String,
    total_invested: Option<f64>,
    dividends_received: Option<f64>,
    market_data_limitations: Vec<MarketDataLimitation>,
}

#[allow(dead_code)]
fn empty_current_positions() -> CurrentPositions {
    CurrentPositions {
        base_currency: BASE_CURRENCY.to_owned(),
        positions: Vec::new(),
        monetary_positions: Vec::new(),
        total_current_value: Some(0.0),
        total_monetary_value: Some(0.0),
        total_value: Some(0.0),
        total_invested: Some(0.0),
        total_dividends: Some(0.0),
        total_open_position_gain_loss: Some(0.0),
        total_open_position_gain_loss_pct: Some(0.0),
        market_data_limitations: Vec::new(),
        monetary_market_data_limitations: Vec::new(),
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
async fn project_holding(
    db: &DatabaseConnection,
    market_data: &MarketData,
    asset: Asset,
    transactions: &[Transaction],
) -> anyhow::Result<HoldingProjection> {
    let mut total_qty = 0.0;
    let mut total_invested = Some(0.0);
    let mut dividends_received = Some(0.0);
    let mut market_data_limitations = Vec::new();

    for transaction in transactions {
        if transaction.is_split() {
            total_qty *= transaction.quantity;
        } else if transaction.is_buy() {
            let native_cost = transaction.quantity * cents_to_f64(transaction.price_cents)
                + cents_to_f64(transaction.fees_cents);
            let (rate, limitations) =
                transaction_exchange_rate(db, market_data, &asset, transaction).await?;
            extend_unique_limitations(&mut market_data_limitations, limitations);
            total_invested = total_invested
                .zip(rate)
                .map(|(cost, rate)| cost + native_cost * rate);
            total_qty += transaction.quantity;
        } else if transaction.is_sell() {
            if total_qty > FLOAT_EPSILON {
                let sold_fraction = (transaction.quantity / total_qty).min(1.0);
                total_invested = total_invested.map(|cost| cost * (1.0 - sold_fraction));
            }
            total_qty -= transaction.quantity;
        } else if transaction.is_dividend() {
            let native_dividend = transaction.quantity * cents_to_f64(transaction.price_cents)
                - cents_to_f64(transaction.fees_cents);
            let (rate, limitations) =
                transaction_exchange_rate(db, market_data, &asset, transaction).await?;
            extend_unique_limitations(&mut market_data_limitations, limitations);
            dividends_received = dividends_received
                .zip(rate)
                .map(|(dividends, rate)| dividends + native_dividend * rate);
        }
    }

    Ok(HoldingProjection {
        asset,
        total_qty,
        earliest_transaction_date: transactions
            .first()
            .context("holding projection has no transactions")?
            .date
            .clone(),
        total_invested,
        dividends_received,
        market_data_limitations,
    })
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn complete_sum(mut values: impl Iterator<Item = Option<f64>>) -> Option<f64> {
    values.try_fold(0.0, |sum, value| value.map(|value| sum + value))
}

fn result_without_nav(current_positions: CurrentPositions) -> PortfolioResult {
    PortfolioResult {
        base_currency: BASE_CURRENCY.to_string(),
        rows: current_positions.positions,
        monetary_positions: current_positions.monetary_positions,
        total_current_value: current_positions.total_current_value,
        total_monetary_value: current_positions.total_monetary_value,
        total_value: current_positions.total_value,
        total_invested: current_positions.total_invested,
        total_dividends: current_positions.total_dividends,
        total_open_position_gain_loss: current_positions.total_open_position_gain_loss,
        total_open_position_gain_loss_pct: current_positions.total_open_position_gain_loss_pct,
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
        nav_market_data_limitations: Vec::new(),
        current_position_market_data_limitations: current_positions.market_data_limitations,
        monetary_market_data_limitations: current_positions.monetary_market_data_limitations,
    }
}

#[cfg(any())]
fn empty_result_with_monetary(
    monetary_positions: Vec<MonetaryPosition>,
    total_monetary_value: Option<f64>,
    monetary_market_data_limitations: Vec<MarketDataLimitation>,
) -> PortfolioResult {
    PortfolioResult {
        monetary_positions,
        total_monetary_value,
        monetary_market_data_limitations,
        ..unreachable!()
    }
}

#[cfg(any())]
async fn compute_monetary_positions(
    db: &DatabaseConnection,
    market_data: &MarketData,
    current_date: NaiveDate,
) -> anyhow::Result<(
    Vec<MonetaryPosition>,
    Option<f64>,
    Vec<MarketDataLimitation>,
)> {
    let assets: HashMap<i32, Asset> = asset_repo::find_all(db)
        .await?
        .into_iter()
        .filter(Asset::is_monetary)
        .map(|asset| (asset.id, asset))
        .collect();
    if assets.is_empty() {
        return Ok((Vec::new(), Some(0.0), Vec::new()));
    }

    let mut transactions_by_asset: HashMap<i32, Vec<Transaction>> = HashMap::new();
    let today = format_date(current_date);
    for transaction in transaction_repo::find_all_ordered_by_date(db, None, Some(&today)).await? {
        if assets.contains_key(&transaction.asset_id) {
            transactions_by_asset
                .entry(transaction.asset_id)
                .or_default()
                .push(transaction);
        }
    }

    let open_assets: Vec<&Asset> = assets
        .values()
        .filter(|asset| {
            transactions_by_asset
                .get(&asset.id)
                .is_some_and(|transactions| {
                    Transaction::compute_holdings(transactions) > FLOAT_EPSILON
                })
        })
        .collect();
    if open_assets.is_empty() {
        return Ok((Vec::new(), Some(0.0), Vec::new()));
    }

    let earliest_transaction_date = open_assets
        .iter()
        .filter_map(|asset| transactions_by_asset.get(&asset.id))
        .flat_map(|transactions| {
            transactions
                .iter()
                .map(|transaction| transaction.date.as_str())
        })
        .min()
        .context("open monetary holdings have no transactions")?;
    let end_date = format_date(current_date - Duration::days(1));
    let start_date = earliest_transaction_date.min(end_date.as_str());
    let assets_to_prepare: Vec<Asset> = open_assets.iter().map(|asset| (*asset).clone()).collect();
    market_data
        .prepare_individual_price_market_data(db, &assets_to_prepare, start_date, &end_date)
        .await?;

    let mut positions = Vec::with_capacity(open_assets.len());
    let mut limitations = Vec::new();
    for asset in open_assets {
        let transactions = transactions_by_asset
            .get(&asset.id)
            .context("open monetary holding has no transactions")?;
        let position = compute_monetary_position(db, market_data, asset, transactions).await?;
        extend_unique_limitations(&mut limitations, position.market_data_limitations.clone());
        positions.push(position);
    }

    let total_value = positions
        .iter()
        .map(|position| position.current_value)
        .sum();
    Ok((positions, total_value, limitations))
}

#[cfg(any())]
async fn compute_monetary_position(
    db: &DatabaseConnection,
    market_data: &MarketData,
    asset: &Asset,
    transactions: &[Transaction],
) -> anyhow::Result<MonetaryPosition> {
    let economics = compute_monetary_ledger_economics(db, market_data, asset, transactions).await?;
    let total_qty = economics.total_qty;

    let individual_price = market_data.individual_price_if_available(db, asset).await?;
    let current_value = individual_price
        .native_price
        .zip(individual_price.fx_rate)
        .map(|(price, rate)| total_qty * price * rate);
    let open_position_gain_loss = current_value
        .zip(economics.total_invested)
        .map(|(value, invested)| value - invested);
    let open_position_gain_loss_pct =
        open_position_gain_loss
            .zip(economics.total_invested)
            .map(|(gain_loss, invested)| {
                if invested.abs() < FLOAT_EPSILON {
                    0.0
                } else {
                    (gain_loss / invested) * 100.0
                }
            });

    Ok(MonetaryPosition {
        ticker: asset.ticker.clone(),
        name: asset.name.clone(),
        asset_type: asset.asset_type.clone(),
        currency: asset.currency.clone(),
        morningstar_code: asset.morningstar_code.clone(),
        asset_class: asset.asset_class.clone(),
        equity_style: asset.equity_style.clone(),
        management: asset.management.clone(),
        total_qty,
        avg_cost: economics.avg_cost,
        current_price: individual_price.native_price,
        price_date: individual_price.price_date,
        total_invested: economics.total_invested,
        current_value,
        dividends_received: economics.dividends_received,
        open_position_gain_loss,
        open_position_gain_loss_pct,
        market_data_limitations: individual_price.limitations,
    })
}

#[cfg(any())]
struct MonetaryLedgerEconomics {
    total_qty: f64,
    avg_cost: Option<f64>,
    total_invested: Option<f64>,
    dividends_received: Option<f64>,
}

#[cfg(any())]
async fn compute_monetary_ledger_economics(
    db: &DatabaseConnection,
    market_data: &MarketData,
    asset: &Asset,
    transactions: &[Transaction],
) -> anyhow::Result<MonetaryLedgerEconomics> {
    let mut quantity = 0.0;
    let mut cost_basis = Some(0.0);
    let mut dividends = Some(0.0);

    for transaction in transactions {
        if transaction.is_split() {
            quantity *= transaction.quantity;
        } else if transaction.is_buy() {
            let native_cost = transaction.quantity * cents_to_f64(transaction.price_cents)
                + cents_to_f64(transaction.fees_cents);
            let rate = transaction_exchange_rate(db, market_data, asset, transaction).await?;
            cost_basis = cost_basis
                .zip(rate)
                .map(|(basis, rate)| basis + native_cost * rate);
            quantity += transaction.quantity;
        } else if transaction.is_sell() {
            if quantity > FLOAT_EPSILON {
                let sold_fraction = (transaction.quantity / quantity).min(1.0);
                cost_basis = cost_basis.map(|basis| basis * (1.0 - sold_fraction));
            }
            quantity -= transaction.quantity;
        } else if transaction.is_dividend() {
            let native_dividend = transaction.quantity * cents_to_f64(transaction.price_cents)
                - cents_to_f64(transaction.fees_cents);
            let rate = transaction_exchange_rate(db, market_data, asset, transaction).await?;
            dividends = dividends
                .zip(rate)
                .map(|(total, rate)| total + native_dividend * rate);
        }
    }

    let avg_cost = cost_basis.map(|basis| {
        if quantity > FLOAT_EPSILON {
            basis / quantity
        } else {
            0.0
        }
    });
    Ok(MonetaryLedgerEconomics {
        total_qty: quantity,
        avg_cost,
        total_invested: cost_basis,
        dividends_received: dividends,
    })
}

async fn transaction_exchange_rate(
    db: &DatabaseConnection,
    market_data: &MarketData,
    asset: &Asset,
    transaction: &Transaction,
) -> anyhow::Result<(Option<f64>, Vec<MarketDataLimitation>)> {
    if asset.currency == BASE_CURRENCY {
        Ok((Some(1.0), Vec::new()))
    } else {
        let rate = market_data
            .get_asset_exchange_rate(db, asset, &transaction.date)
            .await?;
        let limitations = if rate.is_none() {
            let date = NaiveDate::parse_from_str(&transaction.date, DATE_FORMAT)
                .context("invalid transaction date")?;
            vec![MarketDataLimitation {
                subject: MarketDataSubject::FxRate {
                    currency: asset.currency.clone(),
                },
                latest_available_date: None,
                requested_end_date: date,
                classification: MarketDataLimitationClassification::ActionableMissingData,
            }]
        } else {
            Vec::new()
        };
        Ok((rate, limitations))
    }
}

#[cfg(any())]
fn collect_market_data_limitations(rows: &[AssetPosition]) -> Vec<MarketDataLimitation> {
    let mut limitations = Vec::new();

    for row in rows {
        for limitation in &row.market_data_limitations {
            if !limitations.contains(limitation) {
                limitations.push(limitation.clone());
            }
        }
    }

    limitations
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

#[cfg(any())]
fn compute_non_monetary_totals(rows: &[AssetPosition]) -> (f64, f64, f64, f64, f64) {
    let included_rows = rows.iter().filter(|row| !row.is_monetary());
    let total_current_value: f64 = included_rows.clone().map(|r| r.current_value).sum();
    let total_invested: f64 = included_rows.clone().map(|r| r.total_invested).sum();
    let total_dividends: f64 = included_rows.map(|r| r.dividends_received).sum();
    let total_open_position_gain_loss = total_current_value - total_invested;
    let total_open_position_gain_loss_pct = if total_invested == 0.0 {
        0.0
    } else {
        (total_open_position_gain_loss / total_invested) * 100.0
    };

    (
        total_current_value,
        total_invested,
        total_dividends,
        total_open_position_gain_loss,
        total_open_position_gain_loss_pct,
    )
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

#[allow(clippy::too_many_lines)]
#[cfg(any())]
async fn compute_asset_positions(
    db: &DatabaseConnection,
    snapshot_date: &str,
    market_data: &MarketData,
    today: NaiveDate,
) -> anyhow::Result<Vec<AssetPosition>> {
    let asset_snapshots = portfolio_asset_history_repo::find_by_date(db, snapshot_date).await?;
    let snapshots_by_asset: HashMap<i32, _> = asset_snapshots
        .into_iter()
        .map(|snapshot| (snapshot.asset_id, snapshot))
        .collect();

    let today = format_date(today);
    let mut transactions_by_asset: HashMap<i32, Vec<Transaction>> = HashMap::new();
    for transaction in transaction_repo::find_all_ordered_by_date(db, None, Some(&today)).await? {
        transactions_by_asset
            .entry(transaction.asset_id)
            .or_default()
            .push(transaction);
    }
    if transactions_by_asset.is_empty() {
        return Ok(Vec::new());
    }

    let asset_ids: Vec<i32> = transactions_by_asset.keys().copied().collect();
    let assets = asset_repo::find_by_ids(db, asset_ids).await?;
    let mut rows: Vec<AssetPosition> = Vec::new();

    for asset_model in &assets {
        if asset_model.is_monetary() || is_benchmark_ticker(&asset_model.ticker) {
            continue;
        }

        let Some(transactions) = transactions_by_asset.get(&asset_model.id) else {
            continue;
        };
        let net_qty = Transaction::compute_holdings(transactions);
        if net_qty <= FLOAT_EPSILON {
            continue;
        }

        let (current_price, price_date, exchange_rate, market_data_limitations) =
            if let Some(snapshot) = snapshots_by_asset.get(&asset_model.id) {
                let individual_price = market_data
                    .individual_price(
                        db,
                        asset_model,
                        IndividualPriceFallback {
                            native_price: snapshot.closing_price,
                            price_date: snapshot.date.clone(),
                            fx_rate: snapshot.exchange_rate,
                        },
                    )
                    .await?;
                (
                    individual_price.native_price,
                    individual_price.price_date,
                    individual_price.fx_rate,
                    individual_price.limitations,
                )
            } else {
                let individual_price = market_data
                    .individual_price_if_available(db, asset_model)
                    .await?;
                let (Some(current_price), Some(price_date), Some(exchange_rate)) = (
                    individual_price.native_price,
                    individual_price.price_date,
                    individual_price.fx_rate,
                ) else {
                    tracing::warn!(
                        ticker = %asset_model.ticker,
                        "omitting current holding because individual valuation data is unavailable"
                    );
                    continue;
                };
                (
                    current_price,
                    price_date,
                    exchange_rate,
                    individual_price.limitations,
                )
            };

        let mut ledger_qty = 0.0;
        let mut cost_basis = 0.0;
        let mut dividends_received = 0.0;
        for transaction in transactions {
            if transaction.is_split() {
                ledger_qty *= transaction.quantity;
            } else if transaction.is_buy() {
                let native_cost = transaction.quantity * cents_to_f64(transaction.price_cents)
                    + cents_to_f64(transaction.fees_cents);
                let tx_rate = market_data
                    .get_asset_exchange_rate(db, asset_model, &transaction.date)
                    .await?
                    .unwrap_or(exchange_rate);
                cost_basis += native_cost * tx_rate;
                ledger_qty += transaction.quantity;
            } else if transaction.is_sell() {
                if ledger_qty > FLOAT_EPSILON {
                    let sold_fraction = (transaction.quantity / ledger_qty).min(1.0);
                    cost_basis *= 1.0 - sold_fraction;
                }
                ledger_qty -= transaction.quantity;
            } else if transaction.is_dividend() {
                let native_dividend = transaction.quantity * cents_to_f64(transaction.price_cents)
                    - cents_to_f64(transaction.fees_cents);
                let tx_rate = market_data
                    .get_asset_exchange_rate(db, asset_model, &transaction.date)
                    .await?
                    .unwrap_or(exchange_rate);
                dividends_received += native_dividend * tx_rate;
            }
        }

        let avg_cost = cost_basis / net_qty;
        let current_value = net_qty * current_price * exchange_rate;
        let total_invested_for_asset = cost_basis;
        let open_position_gain_loss = current_value - total_invested_for_asset;
        let open_position_gain_loss_pct = if total_invested_for_asset == 0.0 {
            0.0
        } else {
            (open_position_gain_loss / total_invested_for_asset) * 100.0
        };

        rows.push(AssetPosition {
            ticker: asset_model.ticker.clone(),
            name: asset_model.name.clone(),
            asset_type: asset_model.asset_type.clone(),
            currency: asset_model.currency.clone(),
            morningstar_code: asset_model.morningstar_code.clone(),
            asset_class: asset_model.asset_class.clone(),
            equity_style: asset_model.equity_style.clone(),
            management: asset_model.management.clone(),
            total_qty: net_qty,
            avg_cost,
            current_price,
            price_date,
            total_invested: total_invested_for_asset,
            current_value,
            dividends_received,
            open_position_gain_loss,
            open_position_gain_loss_pct,
            market_data_limitations,
        });
    }

    Ok(rows)
}
