use migration::{Migrator, MigratorTrait};
use sea_orm::{ConnectionTrait, Database, DatabaseBackend, Statement};

pub mod common;

#[tokio::test]
async fn reference_migration_copies_legacy_proxy_cache_without_mutating_accounting_data() {
    let db = Database::connect("sqlite::memory:")
        .await
        .expect("in-memory SQLite");
    let migrations = Migrator::migrations().len();
    Migrator::up(&db, Some((migrations - 3) as u32))
        .await
        .expect("schema before reference migration");

    for sql in [
        "INSERT INTO assets (id, ticker, name, asset_type, currency, created_at) VALUES (17, 'ACWI', 'legacy proxy cache identity', 'stock', 'USD', 'legacy-created-at')",
        "INSERT INTO transactions (id, asset_id, tx_type, date, units, unit_price_cents, fees_cents, created_at) VALUES (23, 17, 'buy', '2024-01-02', 2, 10000, 1, 'ledger-created-at')",
        "INSERT INTO daily_asset_prices (id, asset_id, date, closing_price, is_api_failure) VALUES (31, 17, '2024-01-02', 99.5, 0), (32, 17, '2024-01-03', 0.0, 1)",
        "INSERT INTO daily_exchange_rates (from_currency, to_currency, date, rate) VALUES ('USD', 'EUR', '2024-01-02', 0.91)",
        "INSERT INTO portfolio_history (date, cash_balance, asset_value, total_value, outstanding_shares, nav) VALUES ('2024-01-02', 0, 199, 199, 2, 99.5)",
        "INSERT INTO portfolio_asset_history (id, date, asset_id, quantity, closing_price, market_value, exchange_rate) VALUES (41, '2024-01-02', 17, 2, 99.5, 181.09, 0.91)",
    ] {
        db.execute(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await
            .expect("seed pre-migration legacy row");
    }

    Migrator::up(&db, Some(1))
        .await
        .expect("additive reference migration");
    let copied = db
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT date, closing_price, is_api_failure FROM benchmark_reference_prices WHERE reference_id = 'legacy-acwi-etf-proxy' ORDER BY date",
        ))
        .await
        .expect("copied proxy cache");
    assert_eq!(copied.len(), 2);
    assert_eq!(
        copied[0].try_get::<String>("", "date").unwrap(),
        "2024-01-02"
    );
    assert_eq!(copied[0].try_get::<f64>("", "closing_price").unwrap(), 99.5);
    assert!(!copied[0].try_get::<bool>("", "is_api_failure").unwrap());
    assert!(copied[1].try_get::<bool>("", "is_api_failure").unwrap());

    for (sql, expected) in [
        (
            "SELECT CAST(id AS REAL) AS value FROM assets WHERE ticker = 'ACWI'",
            17.0,
        ),
        (
            "SELECT CAST(id AS REAL) AS value FROM transactions WHERE id = 23",
            23.0,
        ),
        (
            "SELECT closing_price AS value FROM daily_asset_prices WHERE id = 31",
            99.5,
        ),
        (
            "SELECT rate AS value FROM daily_exchange_rates WHERE from_currency = 'USD'",
            0.91,
        ),
        (
            "SELECT nav AS value FROM portfolio_history WHERE date = '2024-01-02'",
            99.5,
        ),
        (
            "SELECT market_value AS value FROM portfolio_asset_history WHERE id = 41",
            181.09,
        ),
    ] {
        let value: f64 = db
            .query_one(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await
            .expect("preserved source query")
            .expect("preserved source row")
            .try_get("", "value")
            .expect("preserved source value");
        assert_eq!(value, expected, "preserved value from {sql}");
    }

    Migrator::down(&db, Some(1))
        .await
        .expect("drop reference-only schema");
    let price: f64 = db
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT closing_price AS value FROM daily_asset_prices WHERE id = 31",
        ))
        .await
        .expect("legacy cache remains queryable after down")
        .expect("legacy price remains")
        .try_get("", "value")
        .expect("legacy price value");
    assert_eq!(price, 99.5);
    let nav: f64 = db
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT nav AS value FROM portfolio_history WHERE date = '2024-01-02'",
        ))
        .await
        .expect("NAV remains queryable after down")
        .expect("NAV remains")
        .try_get("", "value")
        .expect("NAV value");
    assert_eq!(nav, 99.5);
}
