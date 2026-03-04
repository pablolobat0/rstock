use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use sea_orm::*;
use std::collections::HashMap;

use crate::db::entities::{
    asset, portfolio_asset_history,
    transaction::{self, Entity as Transaction},
};
use crate::models::{AssetPosition, PortfolioResult, PortfolioSummary};
use crate::services::nav;
use crate::services::price::PriceFetcher;

pub async fn get_portfolio(db: &DatabaseConnection) -> anyhow::Result<PortfolioResult> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    // Read today's asset snapshots (built by get_portfolio_summary)
    let asset_snapshots = portfolio_asset_history::Entity::find()
        .filter(portfolio_asset_history::Column::Date.eq(&today))
        .all(db)
        .await?;

    if asset_snapshots.is_empty() {
        return Ok(PortfolioResult {
            rows: Vec::new(),
            total_invested: 0.0,
            total_current_value: 0.0,
            total_gain_loss: 0.0,
            total_gain_loss_pct: 0.0,
        });
    }

    // Load asset metadata
    let asset_ids: Vec<i32> = asset_snapshots.iter().map(|s| s.asset_id).collect();
    let assets = asset::Entity::find()
        .filter(asset::Column::Id.is_in(asset_ids))
        .all(db)
        .await?;
    let asset_map: HashMap<i32, &asset::Model> = assets.iter().map(|a| (a.id, a)).collect();

    let mut rows: Vec<AssetPosition> = Vec::new();

    for snap in &asset_snapshots {
        let asset_model = match asset_map.get(&snap.asset_id) {
            Some(a) => a,
            None => continue,
        };

        // Compute avg_cost from transactions
        let transactions = transaction::Entity::find()
            .filter(transaction::Column::AssetId.eq(snap.asset_id))
            .all(db)
            .await?;

        let total_cost: f64 = transactions
            .iter()
            .map(|t| t.quantity * (t.price_cents as f64 / 100.0) + (t.fees_cents as f64 / 100.0))
            .sum();
        let total_qty: f64 = transactions.iter().map(|t| t.quantity).sum();
        let avg_cost = if total_qty > 0.0 {
            total_cost / total_qty
        } else {
            0.0
        };

        let current_value = snap.market_value;
        let gain_loss = current_value - total_cost;
        let gain_loss_pct = if total_cost != 0.0 {
            (gain_loss / total_cost) * 100.0
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
            current_price: snap.closing_price,
            total_invested: total_cost,
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
    let today_str = today.format("%Y-%m-%d").to_string();

    let latest_snapshot = nav::get_latest_snapshot(db).await?;
    match &latest_snapshot {
        Some(snap) if snap.date == today_str => {
            // Already up to date, skip rebuild
        }
        Some(snap) => {
            // Rebuild from day after latest
            let latest_date = NaiveDate::parse_from_str(&snap.date, "%Y-%m-%d")
                .context("invalid latest snapshot date")?;
            let next_day = latest_date + chrono::Duration::days(1);
            nav::rebuild_portfolio_history(db, next_day, Some(snap), price_fetcher).await?;
        }
        None => {
            // No history at all, full rebuild from first transaction date
            let first_tx = Transaction::find()
                .order_by_asc(transaction::Column::Date)
                .one(db)
                .await?;
            if let Some(tx) = first_tx {
                let start = NaiveDate::parse_from_str(&tx.date, "%Y-%m-%d")
                    .context("invalid first transaction date")?;
                nav::rebuild_portfolio_history(db, start, None, price_fetcher).await?;
            }
        }
    }

    let current_snapshot = nav::get_latest_snapshot(db).await?;
    let current_snapshot = match current_snapshot {
        Some(s) => s,
        None => return Ok(None),
    };
    let current_nav = current_snapshot.nav;

    // Daily change: compare to yesterday's snapshot
    let yesterday = (today - chrono::Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let (daily_change, daily_change_pct) =
        if let Some(prev) = nav::get_snapshot_at_or_before(db, &yesterday).await? {
            if prev.date != current_snapshot.date && prev.total_value > 0.0 {
                let change = current_snapshot.total_value - prev.total_value;
                let change_pct = (change / prev.total_value) * 100.0;
                (Some(change), Some(change_pct))
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

    // Inception date
    let inception_date = nav::get_earliest_snapshot(db).await?.map(|s| s.date);

    // Calculate returns for each period
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
    let snapshot = match nav::get_snapshot_at_or_before(db, target_date).await? {
        Some(s) => s,
        None if fallback_to_inception => match nav::get_earliest_snapshot(db).await? {
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
