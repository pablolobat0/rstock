use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate};
use sea_orm::DatabaseConnection;
use std::collections::HashMap;

use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
    transaction_repo,
};
use crate::models::{cents_to_f64, AssetPosition, PortfolioResult, PortfolioSummary};
use crate::services::exchange_rates::{self, BASE_CURRENCY};
use crate::services::nav;
use crate::services::price::PriceFetcher;

pub async fn get_portfolio(db: &DatabaseConnection) -> anyhow::Result<PortfolioResult> {
    let latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    let snapshot_date = match &latest_snapshot {
        Some(s) => s.date.clone(),
        None => {
            return Ok(PortfolioResult {
                rows: Vec::new(),
                total_invested: 0.0,
                total_current_value: 0.0,
                total_gain_loss: 0.0,
                total_gain_loss_pct: 0.0,
            });
        }
    };

    let asset_snapshots = portfolio_asset_history_repo::find_by_date(db, &snapshot_date).await?;

    if asset_snapshots.is_empty() {
        return Ok(PortfolioResult {
            rows: Vec::new(),
            total_invested: 0.0,
            total_current_value: 0.0,
            total_gain_loss: 0.0,
            total_gain_loss_pct: 0.0,
        });
    }

    let yesterday = (chrono::Local::now().date_naive() - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();

    let asset_ids: Vec<i32> = asset_snapshots.iter().map(|s| s.asset_id).collect();
    let assets = asset_repo::find_by_ids(db, asset_ids).await?;
    let asset_map: HashMap<i32, _> = assets.iter().map(|a| (a.id, a)).collect();

    let mut rows: Vec<AssetPosition> = Vec::new();

    for snap in &asset_snapshots {
        let asset_model = match asset_map.get(&snap.asset_id) {
            Some(a) => a,
            None => continue,
        };

        // Get the latest exchange rate for this asset's currency
        let exchange_rate = if asset_model.currency != BASE_CURRENCY {
            let pair = exchange_rates::currency_pair(&asset_model.currency);
            exchange_rates::get_exchange_rate(db, &pair, &yesterday)
                .await?
                .unwrap_or(snap.exchange_rate)
        } else {
            1.0
        };

        // Compute avg_cost from transactions, converted to EUR
        let transactions = transaction_repo::find_by_asset_id(db, snap.asset_id).await?;

        let mut total_cost_eur = 0.0;
        let total_qty: f64 = transactions.iter().map(|t| t.quantity).sum();

        for t in &transactions {
            let tx_cost =
                t.quantity * cents_to_f64(t.price_cents) + cents_to_f64(t.fees_cents);
            if asset_model.currency != BASE_CURRENCY {
                let pair = exchange_rates::currency_pair(&asset_model.currency);
                let tx_rate = exchange_rates::get_exchange_rate(db, &pair, &t.date)
                    .await?
                    .unwrap_or(exchange_rate);
                total_cost_eur += tx_cost * tx_rate;
            } else {
                total_cost_eur += tx_cost;
            }
        }

        let avg_cost = if total_qty > 0.0 {
            total_cost_eur / total_qty
        } else {
            0.0
        };

        // Get each asset's own latest price and its date
        let (current_price, price_date) =
            match daily_price_repo::find_price_and_date_at_or_before(db, snap.asset_id, &yesterday)
                .await?
            {
                Some((price, date)) => (price, date),
                None => (snap.closing_price, snap.date.clone()),
            };

        let current_value = snap.quantity * current_price * exchange_rate;
        let gain_loss = current_value - total_cost_eur;
        let gain_loss_pct = if total_cost_eur != 0.0 {
            (gain_loss / total_cost_eur) * 100.0
        } else {
            0.0
        };

        rows.push(AssetPosition {
            ticker: asset_model.ticker.clone(),
            name: asset_model.name.clone(),
            asset_type: asset_model.asset_type.clone(),
            currency: asset_model.currency.clone(),
            total_qty: snap.quantity,
            avg_cost,
            current_price,
            price_date,
            total_invested: total_cost_eur,
            current_value,
            gain_loss,
            gain_loss_pct,
        });
    }

    let total_current_value: f64 = rows.iter().map(|r| r.current_value).sum();
    let total_invested: f64 = rows.iter().map(|r| r.total_invested).sum();
    let total_gain_loss = total_current_value - total_invested;
    let total_gain_loss_pct = if total_invested != 0.0 {
        (total_gain_loss / total_invested) * 100.0
    } else {
        0.0
    };

    Ok(PortfolioResult {
        rows,
        total_invested,
        total_current_value,
        total_gain_loss,
        total_gain_loss_pct,
    })
}

pub async fn get_portfolio_summary(
    db: &DatabaseConnection,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<Option<PortfolioSummary>> {
    let today = chrono::Local::now().date_naive();
    let yesterday = today - Duration::days(1);
    let yesterday_str = yesterday.format("%Y-%m-%d").to_string();

    let latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    match &latest_snapshot {
        Some(snap) if snap.date >= yesterday_str => {
            // Already up to date, skip rebuild
        }
        Some(snap) => {
            let latest_date = NaiveDate::parse_from_str(&snap.date, "%Y-%m-%d")
                .context("invalid latest snapshot date")?;
            let next_day = latest_date + chrono::Duration::days(1);
            nav::rebuild_portfolio_history(db, next_day, yesterday, Some(snap), price_fetcher)
                .await?;
        }
        None => {
            if let Some(tx) = transaction_repo::find_earliest(db).await? {
                let start = NaiveDate::parse_from_str(&tx.date, "%Y-%m-%d")
                    .context("invalid first transaction date")?;
                nav::rebuild_portfolio_history(db, start, yesterday, None, price_fetcher).await?;
            }
        }
    }

    let current_snapshot = match portfolio_history_repo::find_latest(db).await? {
        Some(s) => s,
        None => return Ok(None),
    };
    let current_nav = current_snapshot.nav;
    let snapshot_date = current_snapshot.date.clone();

    // Daily change: compare to the day before the latest snapshot
    let snap_date =
        NaiveDate::parse_from_str(&snapshot_date, "%Y-%m-%d").context("invalid snapshot date")?;
    let prev_day = (snap_date - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let (daily_change, daily_change_pct) =
        if let Some(prev) = portfolio_history_repo::find_at_or_before(db, &prev_day).await? {
            if prev.total_value > 0.0 {
                let change = current_snapshot.total_value - prev.total_value;
                let change_pct = (change / prev.total_value) * 100.0;
                (Some(change), Some(change_pct))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

    let inception_date = portfolio_history_repo::find_earliest(db)
        .await?
        .map(|s| s.date);

    let ytd_date = NaiveDate::from_ymd_opt(today.year(), 1, 1)
        .unwrap()
        .format("%Y-%m-%d")
        .to_string();
    let one_year_date = (today - chrono::Duration::days(365))
        .format("%Y-%m-%d")
        .to_string();
    let three_year_date = (today - chrono::Duration::days(1095))
        .format("%Y-%m-%d")
        .to_string();
    let five_year_date = (today - chrono::Duration::days(1825))
        .format("%Y-%m-%d")
        .to_string();

    let ytd_return = calc_return(db, current_nav, &ytd_date, true, None).await?;
    let one_year_return = calc_return(db, current_nav, &one_year_date, false, None).await?;
    let three_year_return =
        calc_return(db, current_nav, &three_year_date, false, Some(3.0)).await?;
    let five_year_return = calc_return(db, current_nav, &five_year_date, false, Some(5.0)).await?;

    Ok(Some(PortfolioSummary {
        total_value: current_snapshot.total_value,
        nav: current_nav,
        snapshot_date,
        daily_change,
        daily_change_pct,
        inception_date,
        ytd_return,
        one_year_return,
        three_year_return,
        five_year_return,
    }))
}

async fn calc_return(
    db: &DatabaseConnection,
    current_nav: f64,
    target_date: &str,
    fallback_to_inception: bool,
    annualize_years: Option<f64>,
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
        let ret = match annualize_years {
            Some(years) if years > 0.0 => {
                let ratio = current_nav / snapshot.nav;
                (ratio.powf(1.0 / years) - 1.0) * 100.0
            }
            _ => ((current_nav - snapshot.nav) / snapshot.nav) * 100.0,
        };
        Ok(Some(ret))
    } else {
        Ok(None)
    }
}
