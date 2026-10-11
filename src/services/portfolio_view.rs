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
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, FIVE_YEAR_DAYS, FLOAT_EPSILON, MIN_DATA_POINTS, ONE_MONTH_DAYS, ONE_YEAR_DAYS,
    SIX_MONTH_DAYS, THREE_MONTH_DAYS, THREE_YEAR_DAYS,
};
use crate::db::repos::{asset_repo, portfolio_history_repo, transaction_repo};
use crate::models::{
    Asset, AvailablePortfolioPerformance, FactAvailability, MarketDataLimitation,
    NavHistoryRequest, PerformancePeriods, PeriodOutcome, PortfolioPerformance, PortfolioSnapshot,
    PortfolioView,
};
use crate::services::market_data::MarketData;
use crate::services::{ledger, metrics, nav, portfolio_inventory};

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
/// Available performance carries the YTD, 1Y, 3Y, 5Y, and All period
/// outcomes over the ready NAV history through the Effective valuation date.
/// YTD starts at the first calculable NAV point in the year and falls back to
/// inception when the ready history does not reach the current year; All
/// means since inception. Each period groups its return with its risk facts
/// while every calculated fact keeps independent availability. Reference
/// snapshot dates stay calculation details and are never exposed as facts.
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

    let (performance, ready_snapshots) = if let Some(snapshot) = &readiness.latest_snapshot {
        // The latest Complete NAV snapshot bounds the measurable history,
        // and the inception snapshot is a required lookup: its absence
        // alongside an available snapshot violates the ledger invariant.
        let inception = portfolio_history_repo::find_earliest(db)
            .await?
            .with_context(|| {
                format!(
                    "available NAV snapshot on {} without any NAV snapshot for the inception date",
                    snapshot.date
                )
            })?;
        let ready_snapshots =
            nav::get_ready_portfolio_history(db, &inception.date, &snapshot.date).await?;
        if ready_snapshots.is_empty() {
            anyhow::bail!(
                "available NAV snapshot on {} without ready history from inception {}",
                snapshot.date,
                inception.date
            );
        }

        let performance = PortfolioPerformance::Available(Box::new(
            available_performance(
                db,
                market_data,
                snapshot,
                readiness.market_data_limitations.clone(),
                &ready_snapshots,
            )
            .await?,
        ));
        (performance, ready_snapshots)
    } else {
        let performance = compose_performance_without_snapshot(
            db,
            &inventory,
            market_data.today(),
            readiness.market_data_limitations,
        )
        .await?;
        (performance, Vec::new())
    };

    // The requested ready NAV history is part of the same successful
    // outcome, so it is derived from the already ready series before the view
    // is returned instead of after presentation began.
    let nav_history = requested_nav_history(&ready_snapshots, market_data, nav_history_request)?;

    Ok(PortfolioView {
        inventory,
        performance,
        nav_history,
    })
}

/// Assembles the available performance facts from the ready NAV history that
/// readiness produced. A Complete NAV snapshot guarantees the Effective
/// valuation date, synchronized value, NAV, inception date, and all five
/// period outcomes.
///
/// Benchmark data preparation is tolerant: missing or stale benchmark data
/// removes only the benchmark-dependent facts and is reported through the
/// benchmark-dependent risk limitations, while portfolio-only facts stay
/// computed from the ready NAV series.
async fn available_performance(
    db: &DatabaseConnection,
    market_data: &MarketData,
    snapshot: &PortfolioSnapshot,
    nav_history_limitations: Vec<MarketDataLimitation>,
    series: &[PortfolioSnapshot],
) -> anyhow::Result<AvailablePortfolioPerformance> {
    let effective_date = NaiveDate::parse_from_str(&snapshot.date, crate::constants::DATE_FORMAT)
        .context("invalid effective snapshot date")?;
    let inception_date = series[0].date.clone();

    let benchmark = market_data
        .benchmark_risk_market_data(db, &inception_date, &snapshot.date)
        .await?;
    let benchmark_returns: HashMap<String, f64> = weekday_log_returns(&benchmark.benchmark_series)?
        .into_iter()
        .collect();
    let periods = compose_periods(
        series,
        &benchmark_returns,
        market_data.today(),
        effective_date,
    )?;

    Ok(AvailablePortfolioPerformance {
        effective_valuation_date: snapshot.date.clone(),
        synchronized_value: snapshot.total_value,
        nav: snapshot.nav,
        inception_date,
        periods,
        nav_history_limitations,
        benchmark_risk_limitations: benchmark.limitations,
    })
}

/// Maps NAV readiness without a Complete NAV snapshot and the inventory's
/// measurable performance-holdings scope onto the explicit core performance
/// state.
async fn compose_performance_without_snapshot(
    db: &DatabaseConnection,
    inventory: &crate::models::PortfolioInventory,
    today: NaiveDate,
    nav_history_limitations: Vec<MarketDataLimitation>,
) -> anyhow::Result<PortfolioPerformance> {
    if !inventory.performance_holdings.positions.is_empty()
        || has_historical_performance_holdings(db, &format_date(today - Duration::days(1))).await?
    {
        Ok(PortfolioPerformance::Unavailable {
            nav_history_limitations,
        })
    } else {
        Ok(PortfolioPerformance::NotApplicable)
    }
}

/// Composes the YTD, 1Y, 3Y, 5Y, and All period outcomes over the ready NAV
/// series through the Effective valuation date.
fn compose_periods(
    series: &[PortfolioSnapshot],
    benchmark_returns: &HashMap<String, f64>,
    today: NaiveDate,
    effective_date: NaiveDate,
) -> anyhow::Result<PerformancePeriods> {
    let year_start =
        NaiveDate::from_ymd_opt(today.year(), 1, 1).context("Jan 1 is always valid")?;
    Ok(PerformancePeriods {
        ytd: period_outcome(ytd_series(series, year_start), benchmark_returns, false)?,
        one_year: period_outcome(
            trailing_series(series, effective_date, ONE_YEAR_DAYS),
            benchmark_returns,
            false,
        )?,
        three_years: period_outcome(
            trailing_series(series, effective_date, THREE_YEAR_DAYS),
            benchmark_returns,
            true,
        )?,
        five_years: period_outcome(
            trailing_series(series, effective_date, FIVE_YEAR_DAYS),
            benchmark_returns,
            true,
        )?,
        all: period_outcome(Some(series), benchmark_returns, true)?,
    })
}

/// YTD series slice: from the first ready snapshot on or after January 1 of
/// the current year, falling back to the inception snapshot when the ready
/// history does not reach the current year.
fn ytd_series(series: &[PortfolioSnapshot], year_start: NaiveDate) -> Option<&[PortfolioSnapshot]> {
    let year_start = format_date(year_start);
    series
        .iter()
        .position(|snapshot| snapshot.date.as_str() >= year_start.as_str())
        .map_or_else(|| Some(series), |index| Some(&series[index..]))
}

/// Trailing-period series slice: from the latest ready snapshot at or before
/// `effective_date - days`. `None` when the ready history does not reach back
/// that far.
fn trailing_series(
    series: &[PortfolioSnapshot],
    effective_date: NaiveDate,
    days: i64,
) -> Option<&[PortfolioSnapshot]> {
    let target = format_date(effective_date - Duration::days(days));
    series
        .iter()
        .rposition(|snapshot| snapshot.date.as_str() <= target.as_str())
        .map(|index| &series[index..])
}

/// One period's cohesive return and risk facts over its ready NAV series
/// slice.
///
/// Volatility, maximum drawdown, Sharpe, and Sortino derive from the series
/// alone (weekday NAV log returns and NAV values); beta aligns the weekday
/// NAV returns with benchmark observations. Insufficient observations and
/// mathematically undefined metrics are not applicable, while a missing
/// benchmark input makes only beta unavailable.
fn period_outcome(
    series: Option<&[PortfolioSnapshot]>,
    benchmark_returns: &HashMap<String, f64>,
    annualized: bool,
) -> anyhow::Result<PeriodOutcome> {
    let Some(series) = series else {
        // The ready history does not reach the period start.
        return Ok(PeriodOutcome {
            return_pct: FactAvailability::NotApplicable,
            volatility: FactAvailability::NotApplicable,
            max_drawdown: FactAvailability::NotApplicable,
            beta: FactAvailability::NotApplicable,
            sharpe: FactAvailability::NotApplicable,
            sortino: FactAvailability::NotApplicable,
        });
    };

    let weekday_returns = {
        let nav_series: Vec<(String, f64)> = series
            .iter()
            .map(|snapshot| (snapshot.date.clone(), snapshot.nav))
            .collect();
        weekday_log_returns(&nav_series)?
    };
    let weekday_return_values: Vec<f64> = weekday_returns.iter().map(|(_, value)| *value).collect();
    let nav_values: Vec<f64> = series.iter().map(|snapshot| snapshot.nav).collect();

    Ok(PeriodOutcome {
        return_pct: period_return(series, annualized),
        volatility: computed_fact(&weekday_return_values, metrics::compute_volatility),
        max_drawdown: computed_fact(&nav_values, metrics::compute_max_drawdown),
        beta: beta_fact(&weekday_returns, benchmark_returns),
        sharpe: computed_fact(&weekday_return_values, metrics::compute_sharpe),
        sortino: computed_fact(&weekday_return_values, metrics::compute_sortino),
    })
}

/// Return over the ready series slice as a percentage of the baseline NAV.
/// A single-point series has no measurable interval, and a baseline NAV
/// without value makes the return undefined; both are not applicable.
/// YTD and 1Y report simple interval returns; 3Y, 5Y, and All report
/// annualized CAGR values consistent with the existing period conventions.
fn period_return(series: &[PortfolioSnapshot], annualized: bool) -> FactAvailability<f64> {
    let (Some(first), Some(last)) = (series.first(), series.last()) else {
        return FactAvailability::NotApplicable;
    };
    if series.len() < 2 || first.nav <= FLOAT_EPSILON {
        return FactAvailability::NotApplicable;
    }

    if annualized {
        metrics::compute_cagr(&first.date, &last.date, first.nav, last.nav)
            .map_or(FactAvailability::NotApplicable, FactAvailability::Available)
    } else {
        FactAvailability::Available((last.nav - first.nav) / first.nav * 100.0)
    }
}

/// Beta over the weekday NAV returns aligned with benchmark observations.
///
/// The aligned sample pairs each return date with the benchmark return
/// observed on it. Too few aligned observations mean the benchmark input is
/// missing, which makes only beta unavailable; a zero-variance benchmark
/// makes beta mathematically undefined, which is not applicable.
fn beta_fact(
    weekday_returns: &[(String, f64)],
    benchmark_returns: &HashMap<String, f64>,
) -> FactAvailability<f64> {
    if weekday_returns.len() < MIN_DATA_POINTS {
        return FactAvailability::NotApplicable;
    }

    let portfolio_returns: HashMap<String, f64> = weekday_returns.iter().cloned().collect();
    let aligned =
        metrics::align_return_series_with_dates_unfiltered(&portfolio_returns, benchmark_returns);
    if aligned.len() < MIN_DATA_POINTS {
        return FactAvailability::Unavailable;
    }

    let portfolio: Vec<f64> = aligned.iter().map(|(_, portfolio, _)| *portfolio).collect();
    let benchmark: Vec<f64> = aligned.iter().map(|(_, _, benchmark)| *benchmark).collect();
    metrics::compute_beta(&portfolio, &benchmark)
        .map_or(FactAvailability::NotApplicable, FactAvailability::Available)
}

/// Availability of one metric computed from a series. The metric helpers
/// return `None` when the series cannot support the metric (insufficient
/// observations or mathematically undefined math), which is not applicable.
fn computed_fact(values: &[f64], compute: impl Fn(&[f64]) -> Option<f64>) -> FactAvailability<f64> {
    compute(values).map_or(FactAvailability::NotApplicable, FactAvailability::Available)
}

/// Weekday log returns of a dated price series over consecutive
/// observations, keyed by the date each return is observed on. Returns are
/// restricted to weekdays because the series covers contiguous calendar
/// dates; this approximates the completed-weekday trading cadence without
/// depending on any benchmark. Non-finite returns from unusable values are
/// dropped.
fn weekday_log_returns(series: &[(String, f64)]) -> anyhow::Result<Vec<(String, f64)>> {
    let mut returns: Vec<(String, f64)> = Vec::with_capacity(series.len());
    for (date, value) in metrics::compute_log_returns(series) {
        let parsed = NaiveDate::parse_from_str(&date, crate::constants::DATE_FORMAT)
            .with_context(|| format!("invalid ready snapshot date {date}"))?;
        if matches!(parsed.weekday(), Weekday::Sat | Weekday::Sun) || !value.is_finite() {
            continue;
        }
        returns.push((date, value));
    }
    returns.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(returns)
}

/// Reads the ready NAV history the accepted request asks for: a slice of the
/// already ready series starting at the period's first date. Only complete
/// persisted snapshots are part of the series; when performance is not
/// measurable no ready history can exist, so no slicing is needed.
fn requested_nav_history(
    ready_snapshots: &[PortfolioSnapshot],
    market_data: &MarketData,
    request: NavHistoryRequest,
) -> anyhow::Result<Vec<PortfolioSnapshot>> {
    let today = market_data.today();
    let start = match request {
        // No ready snapshot means no ready history at all: an empty start
        // keeps the slice empty for the not-measurable performance states.
        NavHistoryRequest::All => ready_snapshots
            .first()
            .map(|snapshot| snapshot.date.clone())
            .unwrap_or_default(),
        NavHistoryRequest::OneMonth => format_date(today - Duration::days(ONE_MONTH_DAYS)),
        NavHistoryRequest::ThreeMonths => format_date(today - Duration::days(THREE_MONTH_DAYS)),
        NavHistoryRequest::SixMonths => format_date(today - Duration::days(SIX_MONTH_DAYS)),
        NavHistoryRequest::Ytd => format_date(
            NaiveDate::from_ymd_opt(today.year(), 1, 1).context("Jan 1 is always valid")?,
        ),
        NavHistoryRequest::OneYear => format_date(today - Duration::days(ONE_YEAR_DAYS)),
        NavHistoryRequest::ThreeYears => format_date(today - Duration::days(THREE_YEAR_DAYS)),
        NavHistoryRequest::FiveYears => format_date(today - Duration::days(FIVE_YEAR_DAYS)),
    };

    Ok(ready_snapshots
        .iter()
        .filter(|snapshot| snapshot.date.as_str() >= start.as_str())
        .cloned()
        .collect())
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
