use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use sea_orm::*;

use crate::db::entities::{asset, portfolio_asset_history, portfolio_history, transaction};
use crate::services::daily_prices;
use crate::services::price::PriceFetcher;

struct AssetDayValue {
    asset_id: i32,
    quantity: f64,
    closing_price: f64,
    market_value: f64,
}

async fn fill_asset_prices(
    db: &DatabaseConnection,
    assets: &[asset::Model],
    start_date: &str,
    end_date: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<()> {
    for asset in assets {
        if let Err(e) =
            daily_prices::fill_prices_for_range(db, asset, start_date, end_date, price_fetcher)
                .await
        {
            eprintln!("Warning: failed to fill prices for {}: {}", asset.ticker, e);
        }
    }
    Ok(())
}

pub fn process_day_transactions(
    day_txs: &[&transaction::Model],
    holdings: &mut HashMap<i32, f64>,
    outstanding_shares: f64,
    nav: f64,
) -> (f64, f64) {
    let mut os = outstanding_shares;
    let mut current_nav = nav;

    for tx in day_txs {
        let deposit =
            tx.quantity * (tx.price_cents as f64 / 100.0) + (tx.fees_cents as f64 / 100.0);

        if os == 0.0 {
            current_nav = 100.0;
            let shares_issued = deposit / 100.0;
            os = shares_issued;
        } else {
            let shares_issued = deposit / current_nav;
            os += shares_issued;
        }

        *holdings.entry(tx.asset_id).or_insert(0.0) += tx.quantity;
    }

    (os, current_nav)
}

async fn compute_day_asset_values(
    db: &DatabaseConnection,
    holdings: &HashMap<i32, f64>,
    asset_map: &HashMap<i32, &asset::Model>,
    date: &str,
) -> anyhow::Result<(f64, Vec<AssetDayValue>)> {
    // Load existing portfolio_asset_history rows for this date to reuse if valid
    let existing_rows = portfolio_asset_history::Entity::find()
        .filter(portfolio_asset_history::Column::Date.eq(date))
        .all(db)
        .await?;
    let existing_map: HashMap<i32, portfolio_asset_history::Model> =
        existing_rows.into_iter().map(|r| (r.asset_id, r)).collect();

    let mut total_asset_value = 0.0;
    let mut asset_values = Vec::new();

    for (&asset_id, &qty) in holdings {
        if qty <= 0.0 {
            continue;
        }

        // Reuse existing row if quantity matches (asset was not invalidated)
        if let Some(existing) = existing_map.get(&asset_id) {
            if (existing.quantity - qty).abs() < 1e-9 {
                total_asset_value += existing.market_value;
                asset_values.push(AssetDayValue {
                    asset_id,
                    quantity: existing.quantity,
                    closing_price: existing.closing_price,
                    market_value: existing.market_value,
                });
                continue;
            }
        }

        // No existing row or quantity mismatch — compute from daily_asset_prices
        if let Some(asset_model) = asset_map.get(&asset_id) {
            if let Some(closing_price) =
                daily_prices::get_closing_price(db, asset_model, date).await?
            {
                let market_value = qty * closing_price;
                total_asset_value += market_value;
                asset_values.push(AssetDayValue {
                    asset_id,
                    quantity: qty,
                    closing_price,
                    market_value,
                });
            }
        }
    }

    Ok((total_asset_value, asset_values))
}

async fn store_daily_snapshot(
    db: &DatabaseConnection,
    date: &str,
    asset_value: f64,
    outstanding_shares: f64,
    nav: f64,
    asset_values: &[AssetDayValue],
) -> anyhow::Result<()> {
    // Upsert portfolio_history record
    let existing = portfolio_history::Entity::find_by_id(date).one(db).await?;

    let total_value = asset_value;

    if let Some(record) = existing {
        let mut active: portfolio_history::ActiveModel = record.into();
        active.asset_value = Set(asset_value);
        active.total_value = Set(total_value);
        active.outstanding_shares = Set(outstanding_shares);
        active.nav = Set(nav);
        active.update(db).await?;
    } else {
        let record = portfolio_history::ActiveModel {
            date: Set(date.to_owned()),
            asset_value: Set(asset_value),
            total_value: Set(total_value),
            outstanding_shares: Set(outstanding_shares),
            nav: Set(nav),
        };
        record.insert(db).await?;
    }

    // Upsert portfolio_asset_history records
    for av in asset_values {
        let existing = portfolio_asset_history::Entity::find()
            .filter(portfolio_asset_history::Column::Date.eq(date))
            .filter(portfolio_asset_history::Column::AssetId.eq(av.asset_id))
            .one(db)
            .await?;

        if let Some(record) = existing {
            let mut active: portfolio_asset_history::ActiveModel = record.into();
            active.quantity = Set(av.quantity);
            active.closing_price = Set(av.closing_price);
            active.market_value = Set(av.market_value);
            active.update(db).await?;
        } else {
            let record = portfolio_asset_history::ActiveModel {
                date: Set(date.to_owned()),
                asset_id: Set(av.asset_id),
                quantity: Set(av.quantity),
                closing_price: Set(av.closing_price),
                market_value: Set(av.market_value),
                ..Default::default()
            };
            record.insert(db).await?;
        }
    }

    Ok(())
}

pub async fn rebuild_portfolio_history(
    db: &DatabaseConnection,
    start_date: NaiveDate,
    prev_snapshot: Option<&portfolio_history::Model>,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<()> {
    let today = chrono::Local::now().date_naive();
    let start_str = start_date.format("%Y-%m-%d").to_string();
    let today_str = today.format("%Y-%m-%d").to_string();

    // Load all transactions
    let transactions = transaction::Entity::find()
        .order_by_asc(transaction::Column::Date)
        .all(db)
        .await?;

    if transactions.is_empty() {
        return Ok(());
    }

    // Collect needed asset IDs and load asset models
    let needed_ids: HashSet<i32> = transactions.iter().map(|t| t.asset_id).collect();
    let assets: Vec<asset::Model> = asset::Entity::find()
        .filter(asset::Column::Id.is_in(needed_ids))
        .all(db)
        .await?;

    // Initialize state from previous snapshot (or defaults for fresh portfolio)
    let mut is_fresh_portfolio = prev_snapshot.is_none();
    let mut outstanding_shares = prev_snapshot.map(|s| s.outstanding_shares).unwrap_or(0.0);
    let mut nav = prev_snapshot.map(|s| s.nav).unwrap_or(100.0);

    let mut holdings: HashMap<i32, f64> = HashMap::new();
    if let Some(snap) = prev_snapshot {
        let asset_rows = portfolio_asset_history::Entity::find()
            .filter(portfolio_asset_history::Column::Date.eq(&snap.date))
            .all(db)
            .await?;
        for row in asset_rows {
            holdings.insert(row.asset_id, row.quantity);
        }
    }

    // Build transaction map for the rebuild range
    let mut tx_by_date: HashMap<String, Vec<&transaction::Model>> = HashMap::new();
    for tx in &transactions {
        if tx.date >= start_str && tx.date <= today_str {
            tx_by_date.entry(tx.date.clone()).or_default().push(tx);
        }
    }

    let asset_map: HashMap<i32, &asset::Model> = assets.iter().map(|a| (a.id, a)).collect();

    // Fill price caches
    fill_asset_prices(db, &assets, &start_str, &today_str, price_fetcher).await?;

    // Iterate each calendar day
    let mut current = start_date;
    while current <= today {
        let date_str = current.format("%Y-%m-%d").to_string();

        // Process transactions for this day
        if let Some(day_txs) = tx_by_date.get(&date_str) {
            let (new_shares, new_nav) =
                process_day_transactions(day_txs, &mut holdings, outstanding_shares, nav);
            outstanding_shares = new_shares;
            nav = new_nav;
        }

        // First-ever transaction day: store a seed snapshot for (day - 1) with NAV=100
        if is_fresh_portfolio && outstanding_shares > 0.0 {
            let seed_date = (current - chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string();
            store_daily_snapshot(db, &seed_date, 0.0, 0.0, 100.0, &[]).await?;
            is_fresh_portfolio = false;
        }

        if outstanding_shares == 0.0 {
            current += chrono::Duration::days(1);
            continue;
        }

        // Compute EOD values (aggregate + per-asset)
        let (asset_value, asset_values) =
            compute_day_asset_values(db, &holdings, &asset_map, &date_str).await?;

        let total_value = asset_value;
        if outstanding_shares > 0.0 {
            nav = total_value / outstanding_shares;
        }

        // Store to both tables
        store_daily_snapshot(
            db,
            &date_str,
            asset_value,
            outstanding_shares,
            nav,
            &asset_values,
        )
        .await?;

        current += chrono::Duration::days(1);
    }

    Ok(())
}

pub async fn get_latest_snapshot(
    db: &DatabaseConnection,
) -> anyhow::Result<Option<portfolio_history::Model>> {
    let snapshot = portfolio_history::Entity::find()
        .order_by_desc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(snapshot)
}

pub async fn get_snapshot_at_or_before(
    db: &DatabaseConnection,
    date: &str,
) -> anyhow::Result<Option<portfolio_history::Model>> {
    let snapshot = portfolio_history::Entity::find()
        .filter(portfolio_history::Column::Date.lte(date))
        .order_by_desc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(snapshot)
}

pub async fn get_earliest_snapshot(
    db: &DatabaseConnection,
) -> anyhow::Result<Option<portfolio_history::Model>> {
    let snapshot = portfolio_history::Entity::find()
        .order_by_asc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(snapshot)
}
