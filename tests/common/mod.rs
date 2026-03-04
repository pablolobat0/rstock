#![allow(dead_code)]

use std::collections::HashMap;

use migration::{Migrator, MigratorTrait};
use sea_orm::{Database, DatabaseConnection, EntityTrait, Set};

use rstock::db::entities::{asset, daily_asset_price, portfolio_asset_history, portfolio_history, transaction};
use rstock::services::price::PriceFetcher;

pub async fn setup_test_db() -> DatabaseConnection {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("failed to connect to in-memory SQLite");
    Migrator::up(&db, None)
        .await
        .expect("failed to run migrations");
    db
}

pub async fn insert_asset(
    db: &DatabaseConnection,
    ticker: &str,
    name: &str,
    asset_type: &str,
    isin: Option<&str>,
    currency: &str,
) -> i32 {
    let record = asset::ActiveModel {
        ticker: Set(ticker.to_owned()),
        name: Set(name.to_owned()),
        asset_type: Set(asset_type.to_owned()),
        isin: Set(isin.map(|s| s.to_owned())),
        currency: Set(currency.to_owned()),
        created_at: Set("2025-01-01T00:00:00".to_owned()),
        ..Default::default()
    };
    let result = asset::Entity::insert(record)
        .exec(db)
        .await
        .expect("failed to insert asset");
    result.last_insert_id
}

pub async fn insert_transaction(
    db: &DatabaseConnection,
    asset_id: i32,
    date: &str,
    quantity: f64,
    price: f64,
    fees: f64,
) {
    let record = transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set("buy".to_owned()),
        date: Set(date.to_owned()),
        quantity: Set(quantity),
        price_cents: Set((price * 100.0) as i64),
        fees_cents: Set((fees * 100.0) as i64),
        notes: Set(None),
        created_at: Set(format!("{}T00:00:00", date)),
        ..Default::default()
    };
    transaction::Entity::insert(record)
        .exec(db)
        .await
        .expect("failed to insert transaction");
}

pub async fn insert_daily_price(
    db: &DatabaseConnection,
    asset_id: i32,
    date: &str,
    price: f64,
    is_api_failure: bool,
) {
    let record = daily_asset_price::ActiveModel {
        asset_id: Set(asset_id),
        date: Set(date.to_owned()),
        closing_price: Set(price),
        is_api_failure: Set(is_api_failure),
        ..Default::default()
    };
    daily_asset_price::Entity::insert(record)
        .exec(db)
        .await
        .expect("failed to insert daily price");
}

pub async fn get_portfolio_snapshot(
    db: &DatabaseConnection,
    date: &str,
) -> Option<portfolio_history::Model> {
    use sea_orm::QueryFilter;
    use sea_orm::ColumnTrait;
    portfolio_history::Entity::find()
        .filter(portfolio_history::Column::Date.eq(date))
        .one(db)
        .await
        .expect("failed to query portfolio_history")
}

pub async fn get_all_snapshots(db: &DatabaseConnection) -> Vec<portfolio_history::Model> {
    use sea_orm::QueryOrder;
    portfolio_history::Entity::find()
        .order_by_asc(portfolio_history::Column::Date)
        .all(db)
        .await
        .expect("failed to query portfolio_history")
}

pub async fn get_asset_snapshots(
    db: &DatabaseConnection,
    date: &str,
) -> Vec<portfolio_asset_history::Model> {
    use sea_orm::QueryFilter;
    use sea_orm::ColumnTrait;
    use sea_orm::QueryOrder;
    portfolio_asset_history::Entity::find()
        .filter(portfolio_asset_history::Column::Date.eq(date))
        .order_by_asc(portfolio_asset_history::Column::AssetId)
        .all(db)
        .await
        .expect("failed to query portfolio_asset_history")
}

pub struct MockPriceFetcher {
    pub last_prices: HashMap<String, f64>,
    pub historical_prices: HashMap<String, Vec<(String, f64)>>,
}

impl MockPriceFetcher {
    pub fn new() -> Self {
        Self {
            last_prices: HashMap::new(),
            historical_prices: HashMap::new(),
        }
    }
}

#[async_trait::async_trait]
impl PriceFetcher for MockPriceFetcher {
    async fn get_last_price(&self, ticker: &str, _asset_type: &str) -> anyhow::Result<f64> {
        self.last_prices
            .get(ticker)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("mock: no last price for {}", ticker))
    }

    async fn get_historical_prices(
        &self,
        ticker: &str,
        _start: &str,
        _end: &str,
        _asset_type: &str,
    ) -> anyhow::Result<Vec<(String, f64)>> {
        Ok(self
            .historical_prices
            .get(ticker)
            .cloned()
            .unwrap_or_default())
    }
}
