use std::collections::HashMap;

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, is_benchmark_ticker, BASE_CURRENCY, DATE_FORMAT, FIVE_YEAR_DAYS, ONE_YEAR_DAYS,
    THREE_YEAR_DAYS,
};
use crate::db::repos::{
    asset_repo, portfolio_asset_history_repo, portfolio_history_repo, transaction_repo,
};
use crate::models::{
    cents_to_f64, Asset, AssetPosition, PortfolioResult, PortfolioSnapshot, Transaction,
};
use crate::services::nav;
use crate::services::price::PriceFetcher;
use crate::services::{analytics, daily_prices, exchange_rates, metrics};

pub async fn get_portfolio(
    db: &DatabaseConnection,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<PortfolioResult> {
    trigger_rebuild_if_needed(db, price_fetcher).await?;

    let latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    let Some(current_snapshot) = &latest_snapshot else {
        return Ok(empty_result());
    };

    let snapshot_date = current_snapshot.date.clone();
    let current_nav = current_snapshot.nav;
    let total_value = current_snapshot.total_value;

    let today = chrono::Local::now().date_naive();
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

    let (ytd_metrics, one_year_metrics, three_year_metrics, five_year_metrics) =
        analytics::compute_all_period_metrics(
            db,
            &snapshot_date,
            &ytd_date,
            &one_year_date,
            &three_year_date,
            &five_year_date,
            price_fetcher,
        )
        .await?;

    let rows = compute_asset_positions(db, &snapshot_date, price_fetcher).await?;

    let total_current_value: f64 = rows.iter().map(|r| r.current_value).sum();
    let total_invested: f64 = rows.iter().map(|r| r.total_invested).sum();
    let total_dividends: f64 = rows.iter().map(|r| r.dividends_received).sum();
    let total_gain_loss = total_current_value + total_dividends - total_invested;
    let total_gain_loss_pct = if total_invested == 0.0 {
        0.0
    } else {
        (total_gain_loss / total_invested) * 100.0
    };

    Ok(PortfolioResult {
        rows,
        total_invested,
        total_current_value,
        total_dividends,
        total_gain_loss,
        total_gain_loss_pct,
        snapshot_date: Some(snapshot_date),
        nav: Some(current_nav),
        daily_change,
        daily_change_pct,
        inception_date,
        ytd_return,
        one_year_return,
        three_year_return,
        five_year_return,
        ytd_metrics,
        one_year_metrics,
        three_year_metrics,
        five_year_metrics,
    })
}

pub async fn get_asset_positions(
    db: &DatabaseConnection,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<PortfolioResult> {
    trigger_rebuild_if_needed(db, price_fetcher).await?;

    let latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    let snapshot_date = match &latest_snapshot {
        Some(s) => s.date.clone(),
        None => return Ok(empty_result()),
    };

    let rows = compute_asset_positions(db, &snapshot_date, price_fetcher).await?;

    let total_current_value: f64 = rows.iter().map(|r| r.current_value).sum();
    let total_invested: f64 = rows.iter().map(|r| r.total_invested).sum();
    let total_dividends: f64 = rows.iter().map(|r| r.dividends_received).sum();
    let total_gain_loss = total_current_value + total_dividends - total_invested;
    let total_gain_loss_pct = if total_invested == 0.0 {
        0.0
    } else {
        (total_gain_loss / total_invested) * 100.0
    };

    Ok(PortfolioResult {
        rows,
        total_invested,
        total_current_value,
        total_dividends,
        total_gain_loss,
        total_gain_loss_pct,
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
    })
}

pub async fn trigger_rebuild_if_needed(
    db: &DatabaseConnection,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<()> {
    let yesterday = chrono::Local::now().date_naive() - Duration::days(1);
    let yesterday_str = format_date(yesterday);

    let latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    match &latest_snapshot {
        Some(snap) if snap.date >= yesterday_str => {}
        Some(snap) => {
            let latest_date = NaiveDate::parse_from_str(&snap.date, DATE_FORMAT)
                .context("invalid latest snapshot date")?;
            let next_day = latest_date + chrono::Duration::days(1);
            nav::rebuild_portfolio_history(db, next_day, yesterday, Some(snap), price_fetcher)
                .await?;
        }
        None => {
            if let Some(tx) = transaction_repo::find_earliest(db).await? {
                let start = NaiveDate::parse_from_str(&tx.date, DATE_FORMAT)
                    .context("invalid first transaction date")?;
                nav::rebuild_portfolio_history(db, start, yesterday, None, price_fetcher).await?;
            }
        }
    }
    Ok(())
}

pub async fn list_assets(db: &DatabaseConnection) -> anyhow::Result<Vec<Asset>> {
    asset_repo::find_all(db).await
}

pub async fn get_nav_snapshots(
    db: &DatabaseConnection,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<PortfolioSnapshot>> {
    portfolio_history_repo::find_between(db, start_date, end_date).await
}

pub async fn get_inception_date(db: &DatabaseConnection) -> anyhow::Result<Option<String>> {
    let earliest = portfolio_history_repo::find_earliest(db).await?;
    Ok(earliest.map(|s| s.date))
}

// --- Private helpers ---

fn empty_result() -> PortfolioResult {
    PortfolioResult {
        rows: Vec::new(),
        total_invested: 0.0,
        total_current_value: 0.0,
        total_dividends: 0.0,
        total_gain_loss: 0.0,
        total_gain_loss_pct: 0.0,
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

#[allow(clippy::too_many_lines)]
async fn compute_asset_positions(
    db: &DatabaseConnection,
    snapshot_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Vec<AssetPosition>> {
    let asset_snapshots = portfolio_asset_history_repo::find_by_date(db, snapshot_date).await?;
    if asset_snapshots.is_empty() {
        return Ok(Vec::new());
    }

    let today = chrono::Local::now().date_naive();
    let today_str = format_date(today);
    let yesterday = format_date(today - chrono::Duration::days(1));

    let asset_ids: Vec<i32> = asset_snapshots.iter().map(|s| s.asset_id).collect();
    let assets = asset_repo::find_by_ids(db, asset_ids).await?;
    let asset_map: HashMap<i32, _> = assets.iter().map(|a| (a.id, a)).collect();

    let live_prices =
        daily_prices::fetch_live_prices_batch(&assets, &today_str, price_fetcher).await;
    let live_rates =
        exchange_rates::fetch_live_rates_batch(&assets, &today_str, price_fetcher).await;

    let mut rows: Vec<AssetPosition> = Vec::new();

    for snap in &asset_snapshots {
        let Some(asset_model) = asset_map.get(&snap.asset_id) else {
            continue;
        };

        if is_benchmark_ticker(&asset_model.ticker) {
            continue;
        }

        let exchange_rate = if asset_model.currency == BASE_CURRENCY {
            1.0
        } else {
            let pair = exchange_rates::currency_pair(&asset_model.currency);
            if let Some(&live_rate) = live_rates.get(&pair) {
                live_rate
            } else {
                exchange_rates::get_exchange_rate(db, &pair, &yesterday)
                    .await?
                    .unwrap_or(snap.exchange_rate)
            }
        };

        let transactions = transaction_repo::find_by_asset_id(db, snap.asset_id).await?;

        let mut total_buy_cost_eur = 0.0;
        let total_buy_qty: f64 = transactions
            .iter()
            .filter(|t| t.is_buy())
            .map(|t| t.quantity)
            .sum();
        let net_qty: f64 = transactions.iter().map(Transaction::signed_quantity).sum();

        for t in transactions.iter().filter(|t| t.is_buy()) {
            let tx_cost = t.quantity * cents_to_f64(t.price_cents) + cents_to_f64(t.fees_cents);
            if asset_model.currency == BASE_CURRENCY {
                total_buy_cost_eur += tx_cost;
            } else {
                let pair = exchange_rates::currency_pair(&asset_model.currency);
                let tx_rate = exchange_rates::get_exchange_rate(db, &pair, &t.date)
                    .await?
                    .unwrap_or(exchange_rate);
                total_buy_cost_eur += tx_cost * tx_rate;
            }
        }

        let avg_cost = if total_buy_qty > 0.0 {
            total_buy_cost_eur / total_buy_qty
        } else {
            0.0
        };

        let mut dividends_received = 0.0;
        for t in transactions.iter().filter(|t| t.is_dividend()) {
            let div_amount = t.quantity * cents_to_f64(t.price_cents) - cents_to_f64(t.fees_cents);
            if asset_model.currency == BASE_CURRENCY {
                dividends_received += div_amount;
            } else {
                let pair = exchange_rates::currency_pair(&asset_model.currency);
                let tx_rate = exchange_rates::get_exchange_rate(db, &pair, &t.date)
                    .await?
                    .unwrap_or(exchange_rate);
                dividends_received += div_amount * tx_rate;
            }
        }

        let (current_price, price_date) = if let Some(&live_price) = live_prices.get(&snap.asset_id)
        {
            (live_price, today_str.clone())
        } else {
            match daily_prices::get_price_and_date_at_or_before(db, snap.asset_id, &yesterday)
                .await?
            {
                Some((price, date)) => (price, date),
                None => (snap.closing_price, snap.date.clone()),
            }
        };

        let current_value = snap.quantity * current_price * exchange_rate;
        let total_invested_for_asset = net_qty * avg_cost;
        let gain_loss = current_value + dividends_received - total_invested_for_asset;
        let gain_loss_pct = if total_invested_for_asset == 0.0 {
            0.0
        } else {
            (gain_loss / total_invested_for_asset) * 100.0
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
            total_qty: snap.quantity,
            avg_cost,
            current_price,
            price_date,
            total_invested: total_invested_for_asset,
            current_value,
            dividends_received,
            gain_loss,
            gain_loss_pct,
        });
    }

    Ok(rows)
}
