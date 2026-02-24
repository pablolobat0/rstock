use anyhow::Context;
use chrono::{Datelike, NaiveDate};
use sea_orm::*;

use crate::db::entities::{asset, transaction};
use crate::models::{PortfolioRow, PortfolioSummary};
use crate::services::{nav, price};

pub async fn get_portfolio(db: &DatabaseConnection) -> anyhow::Result<Vec<PortfolioRow>> {
    let assets = asset::Entity::find().all(db).await?;

    let mut rows = Vec::new();

    for asset in assets {
        let transactions = transaction::Entity::find()
            .filter(transaction::Column::AssetId.eq(asset.id))
            .all(db)
            .await?;

        if transactions.is_empty() {
            continue;
        }

        let total_qty: f64 = transactions.iter().map(|t| t.quantity).sum();
        let total_cost: f64 = transactions
            .iter()
            .map(|t| t.quantity * (t.price_cents as f64 / 100.0) + (t.fees_cents as f64 / 100.0))
            .sum();
        let avg_cost = total_cost / total_qty;

        let current_price = match asset.asset_type.as_str() {
            "stock" => price::get_last_price(&asset.ticker).await,
            "fund" | "etf" => match &asset.isin {
                Some(isin) => price::get_last_fund_price(isin).await,
                None => {
                    eprintln!(
                        "Warning: no ISIN for {} ({}), skipping price fetch",
                        asset.ticker, asset.asset_type
                    );
                    continue;
                }
            },
            other => {
                eprintln!("Warning: unknown asset type '{}' for {}", other, asset.ticker);
                continue;
            }
        }
        .context(format!("failed to fetch price for {}", asset.ticker))?;

        let current_value = total_qty * current_price;
        let gain_loss = current_value - total_cost;
        let gain_loss_pct = if total_cost != 0.0 {
            (gain_loss / total_cost) * 100.0
        } else {
            0.0
        };

        let sign = if gain_loss >= 0.0 { "+" } else { "" };

        rows.push(PortfolioRow {
            ticker: asset.ticker,
            name: asset.name,
            asset_type: asset.asset_type,
            currency: asset.currency,
            quantity: format_qty(total_qty),
            avg_cost: format!("{:.2}", avg_cost),
            current_price: format!("{:.2}", current_price),
            total_invested: format!("{:.2}", total_cost),
            current_value: format!("{:.2}", current_value),
            gain_loss: format!("{}{:.2}", sign, gain_loss),
            gain_loss_pct: format!("{}{:.2}%", sign, gain_loss_pct),
        });
    }

    Ok(rows)
}

pub async fn get_portfolio_summary(
    db: &DatabaseConnection,
) -> anyhow::Result<Option<PortfolioSummary>> {
    // Rebuild portfolio history (smart: only fills from last snapshot)
    nav::rebuild_portfolio_history(db, None).await?;

    let current_snapshot = nav::get_latest_snapshot(db).await?;
    let current_snapshot = match current_snapshot {
        Some(s) => s,
        None => return Ok(None),
    };

    let today = chrono::Local::now().date_naive();
    let current_nav = current_snapshot.nav;

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

    let ytd_return = calc_return(db, current_nav, &ytd_date, true).await?;
    let one_year_return = calc_return(db, current_nav, &one_year_date, false).await?;
    let three_year_return = calc_return(db, current_nav, &three_year_date, false).await?;
    let five_year_return = calc_return(db, current_nav, &five_year_date, false).await?;

    Ok(Some(PortfolioSummary {
        total_value: current_snapshot.total_value,
        nav: current_nav,
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
        Ok(Some(
            ((current_nav - snapshot.nav) / snapshot.nav) * 100.0,
        ))
    } else {
        Ok(None)
    }
}

fn format_qty(qty: f64) -> String {
    if qty.fract() == 0.0 {
        format!("{}", qty as i64)
    } else {
        format!("{:.4}", qty)
    }
}
