use std::collections::{HashMap, HashSet};

use anyhow::Context;
use chrono::{Duration, NaiveDate};
use sea_orm::DatabaseConnection;

use crate::constants::{format_date, FLOAT_EPSILON, INITIAL_NAV};
use crate::db::repos::{
    asset_repo, portfolio_asset_history_repo, portfolio_history_repo, transaction_repo,
};
use crate::models::{cents_to_f64, Asset, AssetSnapshot, PortfolioSnapshot, Transaction};
use crate::services::market_data::MarketData;

#[allow(clippy::too_many_lines)]
pub async fn rebuild_portfolio_history(
    db: &DatabaseConnection,
    start_date: NaiveDate,
    end_date: NaiveDate,
    prev_snapshot: Option<&PortfolioSnapshot>,
    market_data: &MarketData,
) -> anyhow::Result<()> {
    tracing::info!(%start_date, %end_date, "rebuilding portfolio history");

    let end_str = format_date(end_date);
    let start_str = format_date(start_date);

    // Get latest snapshot data
    let mut holdings: HashMap<i32, f64> = HashMap::new();
    if let Some(snap) = prev_snapshot {
        let asset_rows = portfolio_asset_history_repo::find_by_date(db, &snap.date).await?;
        for row in asset_rows {
            holdings.insert(row.asset_id, row.quantity);
        }
    }
    let mut is_fresh_portfolio = prev_snapshot.is_none();
    let mut outstanding_shares = prev_snapshot.map_or(0.0, |s| s.outstanding_shares);
    let mut nav = prev_snapshot.map_or(INITIAL_NAV, |s| s.nav);
    // Accumulated cash from dividends: recovered from total_value - asset_value
    let mut accumulated_cash = prev_snapshot.map_or(0.0, |s| s.total_value - s.asset_value);

    let transactions =
        transaction_repo::find_all_ordered_by_date(db, Some(&start_str), Some(&end_str)).await?;

    let needed_ids: HashSet<i32> = holdings
        .keys()
        .copied()
        .chain(transactions.iter().map(|tx| tx.asset_id))
        .collect();

    if needed_ids.is_empty() {
        return Ok(());
    }
    let assets = asset_repo::find_by_ids(db, needed_ids).await?;

    let nav_assets: Vec<Asset> = assets
        .iter()
        .filter(|asset| !asset.is_monetary())
        .cloned()
        .collect();

    let valuation_market_data = market_data
        .prepare_valuation_market_data(db, &nav_assets, &start_str, &end_str)
        .await?;
    let effective_end = valuation_market_data.effective_end;

    let mut tx_by_date: HashMap<String, Vec<&Transaction>> = HashMap::new();
    for tx in &transactions {
        tx_by_date.entry(tx.date.clone()).or_default().push(tx);
    }

    let asset_map: HashMap<i32, &Asset> = assets.iter().map(|a| (a.id, a)).collect();

    // Iterate each calendar day
    let mut current = start_date;
    while current <= effective_end {
        let date_str = format_date(current);

        let day_asset_exchange_rates = market_data
            .get_required_asset_exchange_rates(db, &nav_assets, &date_str)
            .await?;

        // Process transactions for this day
        if let Some(day_txs) = tx_by_date.get(&date_str) {
            let (new_shares, new_nav, dividend_income) = process_day_transactions(
                day_txs,
                &mut holdings,
                outstanding_shares,
                nav,
                &asset_map,
                &day_asset_exchange_rates,
            )?;
            outstanding_shares = new_shares;
            nav = new_nav;
            accumulated_cash += dividend_income;
        }

        if outstanding_shares == 0.0 && is_fresh_portfolio {
            current += chrono::Duration::days(1);
            continue;
        }

        // Compute EOD values (aggregate + per-asset) with currency conversion
        let (asset_value, asset_values) =
            compute_day_asset_values(db, market_data, &holdings, &asset_map, &date_str).await?;

        let total_value = asset_value + accumulated_cash;
        if outstanding_shares > 0.0 {
            nav = total_value / outstanding_shares;
        }

        // First-ever transaction day: store a seed snapshot only after required valuations succeed.
        if is_fresh_portfolio && outstanding_shares > 0.0 {
            let seed_date = format_date(current - chrono::Duration::days(1));
            store_daily_snapshot(db, &seed_date, 0.0, 0.0, 0.0, INITIAL_NAV, &[]).await?;
            is_fresh_portfolio = false;
        }

        store_daily_snapshot(
            db,
            &date_str,
            asset_value,
            total_value,
            outstanding_shares,
            nav,
            &asset_values,
        )
        .await?;

        current += chrono::Duration::days(1);
    }

    Ok(())
}

/// Ensures portfolio history is current through the last completed date.
pub async fn ensure_portfolio_history(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<()> {
    let yesterday = market_data.today() - Duration::days(1);
    let yesterday_str = format_date(yesterday);

    let latest_snapshot = portfolio_history_repo::find_latest(db).await?;
    match &latest_snapshot {
        Some(snapshot) if snapshot.date >= yesterday_str => {}
        Some(snapshot) => {
            let latest_date =
                NaiveDate::parse_from_str(&snapshot.date, crate::constants::DATE_FORMAT)
                    .context("invalid latest snapshot date")?;
            rebuild_portfolio_history(
                db,
                latest_date + Duration::days(1),
                yesterday,
                Some(snapshot),
                market_data,
            )
            .await?;
        }
        None => {
            if let Some(transaction) = transaction_repo::find_earliest(db).await? {
                let start =
                    NaiveDate::parse_from_str(&transaction.date, crate::constants::DATE_FORMAT)
                        .context("invalid first transaction date")?;
                rebuild_portfolio_history(db, start, yesterday, None, market_data).await?;
            }
        }
    }

    Ok(())
}

/// Returns `(outstanding_shares, nav, dividend_income_eur)`.
#[allow(clippy::implicit_hasher)]
pub fn process_day_transactions(
    day_txs: &[&Transaction],
    holdings: &mut HashMap<i32, f64>,
    outstanding_shares: f64,
    nav: f64,
    asset_map: &HashMap<i32, &Asset>,
    day_asset_exchange_rates: &HashMap<i32, f64>,
) -> anyhow::Result<(f64, f64, f64)> {
    let mut os = outstanding_shares;
    let mut current_nav = nav;
    let mut dividend_income = 0.0;

    for tx in day_txs {
        if asset_map
            .get(&tx.asset_id)
            .is_some_and(|asset| asset.is_monetary())
        {
            continue;
        }

        // Convert to base currency
        let rate = asset_map
            .get(&tx.asset_id)
            .map(|asset| {
                day_asset_exchange_rates
                    .get(&asset.id)
                    .copied()
                    .with_context(|| {
                        format!(
                            "missing prepared historical exchange rate for asset {} ({})",
                            asset.ticker, asset.name
                        )
                    })
            })
            .transpose()?
            .unwrap_or(1.0);

        if tx.is_split() {
            *holdings.entry(tx.asset_id).or_insert(0.0) *= tx.quantity;
        } else if tx.is_dividend() {
            // Dividend = income: accumulate as cash, no holdings or shares change
            let amount = tx.quantity * cents_to_f64(tx.price_cents) - cents_to_f64(tx.fees_cents);
            dividend_income += amount * rate;
        } else if tx.is_sell() {
            // Sell = withdrawal: proceeds = qty * price - fees
            let withdrawal =
                tx.quantity * cents_to_f64(tx.price_cents) - cents_to_f64(tx.fees_cents);
            let withdrawal_eur = withdrawal * rate;

            if os > 0.0 && current_nav > 0.0 {
                let shares_redeemed = withdrawal_eur / current_nav;
                os -= shares_redeemed;
                if os < 0.0 {
                    os = 0.0;
                }
            }

            *holdings.entry(tx.asset_id).or_insert(0.0) -= tx.quantity;
        } else {
            // Buy = deposit: cost = qty * price + fees
            let deposit = tx.quantity * cents_to_f64(tx.price_cents) + cents_to_f64(tx.fees_cents);
            let deposit_eur = deposit * rate;

            if os == 0.0 {
                current_nav = INITIAL_NAV;
                let shares_issued = deposit_eur / INITIAL_NAV;
                os = shares_issued;
            } else {
                let shares_issued = deposit_eur / current_nav;
                os += shares_issued;
            }

            *holdings.entry(tx.asset_id).or_insert(0.0) += tx.quantity;
        }
    }

    Ok((os, current_nav, dividend_income))
}

async fn compute_day_asset_values(
    db: &DatabaseConnection,
    market_data: &MarketData,
    holdings: &HashMap<i32, f64>,
    asset_map: &HashMap<i32, &Asset>,
    date: &str,
) -> anyhow::Result<(f64, Vec<AssetSnapshot>)> {
    let existing_rows = portfolio_asset_history_repo::find_by_date(db, date).await?;
    let existing_map: HashMap<i32, AssetSnapshot> =
        existing_rows.into_iter().map(|r| (r.asset_id, r)).collect();

    let mut total_asset_value = 0.0;
    let mut asset_values = Vec::new();

    for (&asset_id, &qty) in holdings {
        if qty <= 0.0 {
            continue;
        }

        let Some(asset_model) = asset_map.get(&asset_id) else {
            continue;
        };
        if asset_model.is_monetary() {
            continue;
        }
        let valuation = market_data
            .get_required_asset_valuation_data(db, asset_model, date)
            .await?;

        // Reuse existing row if quantity and exchange rate match
        if let Some(existing) = existing_map.get(&asset_id) {
            if (existing.quantity - qty).abs() < FLOAT_EPSILON
                && (existing.exchange_rate - valuation.fx_rate).abs() < FLOAT_EPSILON
            {
                total_asset_value += existing.market_value;
                asset_values.push(AssetSnapshot {
                    date: existing.date.clone(),
                    asset_id,
                    quantity: existing.quantity,
                    closing_price: existing.closing_price,
                    market_value: existing.market_value,
                    exchange_rate: existing.exchange_rate,
                });
                continue;
            }
        }

        let market_value = qty * valuation.base_currency_price;
        total_asset_value += market_value;
        asset_values.push(AssetSnapshot {
            date: date.to_owned(),
            asset_id,
            quantity: qty,
            closing_price: valuation.native_price,
            market_value,
            exchange_rate: valuation.fx_rate,
        });
    }

    Ok((total_asset_value, asset_values))
}

async fn store_daily_snapshot(
    db: &DatabaseConnection,
    date: &str,
    asset_value: f64,
    total_value: f64,
    outstanding_shares: f64,
    nav: f64,
    asset_values: &[AssetSnapshot],
) -> anyhow::Result<()> {
    portfolio_history_repo::upsert(
        db,
        &PortfolioSnapshot {
            date: date.to_owned(),
            asset_value,
            total_value,
            outstanding_shares,
            nav,
        },
    )
    .await?;

    for av in asset_values {
        portfolio_asset_history_repo::upsert(db, av).await?;
    }

    Ok(())
}
