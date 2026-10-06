use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, DATE_FORMAT, FIVE_YEAR_DAYS, ONE_YEAR_DAYS, THREE_YEAR_DAYS};
use crate::db::repos::portfolio_history_repo;
use crate::models::{
    CurrentPosition, FactAvailability, InventoryPosition, MarketDataLimitation, PortfolioInventory,
    PortfolioResult,
};
use crate::services::market_data::MarketData;
use crate::services::{analytics, metrics};
use crate::services::{nav, portfolio_inventory};

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
    let inventory = portfolio_inventory::get_portfolio_inventory(db, market_data).await?;
    let mut nav_market_data_limitations = nav_readiness.market_data_limitations;

    let Some(current_snapshot) = &nav_readiness.latest_snapshot else {
        return Ok(result_without_nav(inventory, nav_market_data_limitations));
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

    for limitation in period_metrics.market_data_limitations {
        if !nav_market_data_limitations.contains(&limitation) {
            nav_market_data_limitations.push(limitation);
        }
    }

    let mut result = portfolio_result_from_inventory(inventory);
    result.snapshot_date = Some(snapshot_date);
    result.nav = Some(current_nav);
    result.daily_change = daily_change;
    result.daily_change_pct = daily_change_pct;
    result.inception_date = inception_date;
    result.ytd_return = ytd_return;
    result.one_year_return = one_year_return;
    result.three_year_return = three_year_return;
    result.five_year_return = five_year_return;
    result.ytd_metrics = period_metrics.ytd;
    result.one_year_metrics = period_metrics.one_year;
    result.three_year_metrics = period_metrics.three_year;
    result.five_year_metrics = period_metrics.five_year;
    result.nav_market_data_limitations = nav_market_data_limitations;
    Ok(result)
}

// --- Temporary migration mapping ---
//
// The old flat Portfolio path stays operational until later migration tickets
// switch its callers. It is now a presentation mapping over the focused
// Portfolio inventory seam rather than a second ledger implementation.

fn portfolio_result_from_inventory(inventory: PortfolioInventory) -> PortfolioResult {
    PortfolioResult {
        base_currency: inventory.base_currency,
        rows: inventory
            .performance_holdings
            .positions
            .iter()
            .map(current_position_from_inventory)
            .collect(),
        monetary_positions: inventory
            .monetary_holdings
            .positions
            .iter()
            .map(current_position_from_inventory)
            .collect(),
        total_current_value: fact_value(&inventory.performance_holdings.aggregates.current_value),
        total_monetary_value: fact_value(&inventory.monetary_holdings.aggregates.current_value),
        total_value: fact_value(&inventory.total_value),
        total_invested: fact_value(&inventory.performance_holdings.aggregates.invested_cost),
        total_monetary_invested: fact_value(&inventory.monetary_holdings.aggregates.invested_cost),
        total_dividends: fact_value(&inventory.performance_holdings.aggregates.dividends),
        total_monetary_dividends: fact_value(&inventory.monetary_holdings.aggregates.dividends),
        total_open_position_gain_loss: fact_value(
            &inventory
                .performance_holdings
                .aggregates
                .open_position_gain_loss,
        ),
        total_open_position_gain_loss_pct: fact_value(
            &inventory
                .performance_holdings
                .aggregates
                .open_position_gain_loss_pct,
        ),
        total_monetary_open_position_gain_loss: fact_value(
            &inventory
                .monetary_holdings
                .aggregates
                .open_position_gain_loss,
        ),
        total_monetary_open_position_gain_loss_pct: fact_value(
            &inventory
                .monetary_holdings
                .aggregates
                .open_position_gain_loss_pct,
        ),
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
        current_position_market_data_limitations: inventory
            .performance_holdings
            .market_data_limitations,
        monetary_market_data_limitations: inventory.monetary_holdings.market_data_limitations,
    }
}

fn fact_value<T: Copy>(fact: &FactAvailability<T>) -> Option<T> {
    fact.value().copied()
}

fn current_position_from_inventory(position: &InventoryPosition) -> CurrentPosition {
    let (current_price, price_date) = position
        .individual_price
        .value()
        .map_or((None, None), |price| {
            (Some(price.native_price), Some(price.price_date.clone()))
        });
    CurrentPosition {
        ticker: position.ticker.clone(),
        name: position.name.clone(),
        asset_type: position.asset_type.clone(),
        currency: position.currency.clone(),
        morningstar_code: position.morningstar_code.clone(),
        asset_class: position.asset_class.clone(),
        equity_style: position.equity_style.clone(),
        management: position.management.clone(),
        total_qty: position.quantity,
        avg_cost: fact_value(&position.average_cost),
        current_price,
        price_date,
        total_invested: fact_value(&position.invested_cost),
        current_value: fact_value(&position.current_value),
        dividends_received: fact_value(&position.dividends),
        open_position_gain_loss: fact_value(&position.open_position_gain_loss),
        open_position_gain_loss_pct: fact_value(&position.open_position_gain_loss_pct),
        market_data_limitations: position.market_data_limitations.clone(),
    }
}

fn result_without_nav(
    inventory: PortfolioInventory,
    nav_market_data_limitations: Vec<MarketDataLimitation>,
) -> PortfolioResult {
    let mut result = portfolio_result_from_inventory(inventory);
    result.nav_market_data_limitations = nav_market_data_limitations;
    result
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
