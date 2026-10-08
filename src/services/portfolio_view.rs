//! Portfolio view composition.
//!
//! `get_portfolio_view()` is the Portfolio view Interface: it composes one
//! complete, presentation-neutral [`PortfolioView`] from the focused
//! Portfolio inventory and explicit core Portfolio performance states, and
//! includes the NAV history the presented request needs before the outcome is
//! returned. NAV readiness order, `MarketData` preparation reuse, period date
//! calculation, and ready-history queries stay private to this module. See
//! ADR-0003 for the composed-outcome decision.

use std::collections::HashMap;

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, FIVE_YEAR_DAYS, FLOAT_EPSILON, ONE_MONTH_DAYS, ONE_YEAR_DAYS, SIX_MONTH_DAYS,
    THREE_MONTH_DAYS, THREE_YEAR_DAYS,
};
use crate::db::repos::{asset_repo, portfolio_history_repo, transaction_repo};
use crate::models::{
    Asset, AvailablePortfolioPerformance, MarketDataLimitation, NavHistoryRequest,
    PortfolioPerformance, PortfolioSnapshot, PortfolioView,
};
use crate::services::market_data::MarketData;
use crate::services::{ledger, nav, portfolio_inventory};

/// Composes one complete, presentation-neutral [`PortfolioView`] for the
/// requested NAV history period.
///
/// The outcome always contains the focused Portfolio inventory. Portfolio
/// performance is grouped into the explicit not-applicable, unavailable, and
/// available states: not applicable when there are no current or historical
/// performance holdings to measure, unavailable when performance holdings
/// exist but no Complete NAV snapshot can be produced, and available at the
/// latest Complete NAV snapshot even when limitations prevent advancing its
/// Effective valuation date.
///
/// Expected domain absence becomes an outcome state with its Market data
/// limitations. Database failures, malformed persisted data, invariant
/// violations, and required-lookup failures remain errors and produce no
/// partial Portfolio view.
pub async fn get_portfolio_view(
    db: &DatabaseConnection,
    market_data: &MarketData,
    nav_history_request: NavHistoryRequest,
) -> anyhow::Result<PortfolioView> {
    // NAV readiness first: it prepares and persists the Complete NAV history
    // the performance state and requested history both read from. The
    // inventory projection then reuses the same MarketData instance, whose
    // identical source attempts are shared for the whole command.
    let readiness = nav::ensure_portfolio_history(db, market_data).await?;
    let inventory = portfolio_inventory::get_portfolio_inventory(db, market_data).await?;

    let performance =
        compose_performance_state(db, &inventory, &readiness, market_data.today()).await?;

    // The requested ready NAV history is part of the same successful
    // outcome, so it is loaded before the view is returned instead of after
    // presentation began.
    let nav_history =
        load_requested_nav_history(db, market_data, &performance, nav_history_request).await?;

    Ok(PortfolioView {
        inventory,
        performance,
        nav_history,
    })
}

/// Maps NAV readiness and the inventory's measurable performance-holdings
/// scope onto the explicit core performance state.
async fn compose_performance_state(
    db: &DatabaseConnection,
    inventory: &crate::models::PortfolioInventory,
    readiness: &nav::PortfolioHistoryReadiness,
    today: NaiveDate,
) -> anyhow::Result<PortfolioPerformance> {
    match &readiness.latest_snapshot {
        Some(snapshot) => Ok(PortfolioPerformance::Available(
            available_performance(db, snapshot, readiness.market_data_limitations.clone()).await?,
        )),
        None => {
            if !inventory.performance_holdings.positions.is_empty()
                || has_historical_performance_holdings(db, &format_date(today - Duration::days(1)))
                    .await?
            {
                Ok(PortfolioPerformance::Unavailable {
                    nav_history_limitations: readiness.market_data_limitations.clone(),
                })
            } else {
                Ok(PortfolioPerformance::NotApplicable)
            }
        }
    }
}

/// Assembles the available performance facts from the Complete NAV snapshot
/// that readiness produced. A snapshot guarantees the Effective valuation
/// date, synchronized value, and NAV; the inception date is a required
/// lookup, so its absence is an invariant failure rather than an optional
/// placeholder.
async fn available_performance(
    db: &DatabaseConnection,
    snapshot: &PortfolioSnapshot,
    nav_history_limitations: Vec<MarketDataLimitation>,
) -> anyhow::Result<AvailablePortfolioPerformance> {
    let inception = portfolio_history_repo::find_earliest(db).await?;
    let inception_date = inception.map(|snapshot| snapshot.date).with_context(|| {
        format!(
            "available NAV snapshot on {} without any NAV snapshot for the inception date",
            snapshot.date
        )
    })?;

    Ok(AvailablePortfolioPerformance {
        effective_valuation_date: snapshot.date.clone(),
        synchronized_value: snapshot.total_value,
        nav: snapshot.nav,
        inception_date,
        nav_history_limitations,
    })
}

/// Determines whether the ledger records performance holdings through
/// `yesterday` even though the current inventory shows none. The distinction
/// separates "nothing was there to measure" from "performance holdings exist
/// but no Complete NAV snapshot could be produced".
async fn has_historical_performance_holdings(
    db: &DatabaseConnection,
    yesterday: &str,
) -> anyhow::Result<bool> {
    let transactions =
        transaction_repo::find_all_ordered_by_date(db, None, Some(yesterday)).await?;
    if transactions.is_empty() {
        return Ok(false);
    }
    let asset_ids: Vec<i32> = transactions.iter().map(|tx| tx.asset_id).collect();
    let assets = asset_repo::find_by_ids(db, asset_ids).await?;
    let asset_by_id: HashMap<i32, &Asset> = assets.iter().map(|asset| (asset.id, asset)).collect();
    let mut transactions_by_asset = HashMap::new();

    for transaction in transactions {
        let asset = asset_by_id.get(&transaction.asset_id).with_context(|| {
            format!(
                "transaction id {} references missing asset id {}",
                transaction.id, transaction.asset_id
            )
        })?;
        if !asset.is_monetary() {
            transactions_by_asset
                .entry(transaction.asset_id)
                .or_insert_with(Vec::new)
                .push(transaction);
        }
    }

    for (asset_id, transactions) in transactions_by_asset {
        let replay = ledger::replay_transactions(asset_id, &transactions)
            .map_err(|error| anyhow::anyhow!(error))?;
        if replay.transitions.iter().any(|transition| {
            transition.quantity_before > FLOAT_EPSILON || transition.quantity_after > FLOAT_EPSILON
        }) {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Reads the ready NAV history the accepted request asks for. Only complete
/// persisted snapshots are returned; when performance is not measurable no
/// ready history can exist, so the query is skipped rather than guessed.
async fn load_requested_nav_history(
    db: &DatabaseConnection,
    market_data: &MarketData,
    performance: &PortfolioPerformance,
    request: NavHistoryRequest,
) -> anyhow::Result<Vec<PortfolioSnapshot>> {
    let Some(available) = (match performance {
        PortfolioPerformance::Available(available) => Some(available),
        PortfolioPerformance::Unavailable { .. } | PortfolioPerformance::NotApplicable => None,
    }) else {
        return Ok(Vec::new());
    };

    let start = match request {
        NavHistoryRequest::All => {
            NaiveDate::parse_from_str(&available.inception_date, crate::constants::DATE_FORMAT)
                .context("invalid inception date")?
        }
        NavHistoryRequest::OneMonth => period_start(market_data, ONE_MONTH_DAYS),
        NavHistoryRequest::ThreeMonths => period_start(market_data, THREE_MONTH_DAYS),
        NavHistoryRequest::SixMonths => period_start(market_data, SIX_MONTH_DAYS),
        NavHistoryRequest::Ytd => NaiveDate::from_ymd_opt(market_data.today().year(), 1, 1)
            .context("Jan 1 is always valid")?,
        NavHistoryRequest::OneYear => period_start(market_data, ONE_YEAR_DAYS),
        NavHistoryRequest::ThreeYears => period_start(market_data, THREE_YEAR_DAYS),
        NavHistoryRequest::FiveYears => period_start(market_data, FIVE_YEAR_DAYS),
    };

    nav::get_ready_portfolio_history(db, &format_date(start), &format_date(market_data.today()))
        .await
}

fn period_start(market_data: &MarketData, days: i64) -> NaiveDate {
    market_data.today() - Duration::days(days)
}
