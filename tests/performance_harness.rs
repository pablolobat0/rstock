//! Lightweight, deterministic checks supporting the offline performance gate.

mod common;

use common::{insert_asset, insert_transaction, setup_test_db};
use migration::{Migrator, MigratorTrait};
use rstock::db::repos::transaction_repo;
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

#[tokio::test]
async fn small_behavior_fixture_has_expected_assets_and_ordered_ledger() {
    let db = setup_test_db().await;
    let mut asset_ids = Vec::new();
    for index in 0..5 {
        asset_ids.push(
            insert_asset(
                &db,
                &format!("XBEHAVE{index}"),
                "Behavior fixture",
                "stock",
                "EUR",
            )
            .await,
        );
    }
    for index in 0..100 {
        insert_transaction(
            &db,
            asset_ids[index % asset_ids.len()],
            &format!("2015-{:02}-{:02}", index % 12 + 1, index % 27 + 1),
            1.0,
            100.0,
            0.0,
        )
        .await;
    }

    let transactions = transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .expect("in-memory behavior fixture ledger");
    assert_eq!(transactions.len(), 100);
    assert!(transactions.windows(2).all(|pair| {
        pair[0].date < pair[1].date || (pair[0].date == pair[1].date && pair[0].id < pair[1].id)
    }));
}

#[tokio::test]
async fn representative_transaction_plans_are_available_for_baselining() {
    let db = setup_test_db().await;
    let mut asset_ids = Vec::new();
    for index in 0..50 {
        asset_ids.push(
            insert_asset(
                &db,
                &format!("XPLAN{index:02}"),
                "Plan fixture",
                "stock",
                "EUR",
            )
            .await,
        );
    }
    for index in 0..5_000 {
        insert_transaction(
            &db,
            asset_ids[index % asset_ids.len()],
            &format!(
                "{}-{:02}-{:02}",
                2015 + index % 10,
                index % 12 + 1,
                index % 27 + 1
            ),
            1.0,
            100.0,
            0.0,
        )
        .await;
    }
    let asset_id = asset_ids[0];

    let queries = [
        "EXPLAIN QUERY PLAN SELECT * FROM transactions ORDER BY date ASC, id ASC",
        &format!("EXPLAIN QUERY PLAN SELECT * FROM transactions WHERE asset_id = {asset_id} ORDER BY date ASC, id ASC"),
        &format!("EXPLAIN QUERY PLAN SELECT * FROM transactions WHERE asset_id = {asset_id} AND date <= '2020-12-31' ORDER BY date ASC, id ASC"),
        &format!("EXPLAIN QUERY PLAN SELECT * FROM daily_asset_prices WHERE asset_id = {asset_id} AND date BETWEEN '2020-01-01' AND '2020-12-31' ORDER BY date ASC"),
        "EXPLAIN QUERY PLAN SELECT * FROM daily_exchange_rates WHERE from_currency = 'USD' AND to_currency = 'EUR' AND date <= '2020-12-31' ORDER BY date DESC LIMIT 1",
    ];
    let mut classified = Vec::new();
    for (index, sql) in queries.into_iter().enumerate() {
        let rows = db
            .query_all(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .expect("SQLite should produce a query plan");
        assert!(
            !rows.is_empty(),
            "query plan must contain at least one step"
        );
        let details: Vec<String> = rows
            .iter()
            .map(|row| row.try_get("", "detail").expect("SQLite plan detail"))
            .collect();
        assert!(details
            .iter()
            .any(|detail| detail.contains("SCAN") || detail.contains("SEARCH")));
        let access = if details.iter().any(|detail| detail.contains("SEARCH")) {
            "search"
        } else {
            "scan"
        };
        let uses_index = details.iter().any(|detail| detail.contains("USING INDEX"));
        let uses_temp_sort = details.iter().any(|detail| detail.contains("TEMP B-TREE"));
        if index < 3 {
            let expected_index = if index == 0 {
                "idx_transactions_date_id"
            } else {
                "idx_transactions_asset_date_id"
            };
            assert!(
                details.iter().any(|detail| detail.contains(expected_index)),
                "transaction plan should use {expected_index}: {details:?}"
            );
            assert!(
                !uses_temp_sort,
                "transaction plan should not require a temporary sort: {details:?}"
            );
        }
        classified.push((access, uses_index, uses_temp_sort));
    }
    assert_eq!(classified.len(), 5);
    println!("transaction_query_plans={classified:?}");
    assert_eq!(
        classified
            .iter()
            .filter(|(_, _, uses_temp_sort)| *uses_temp_sort)
            .count(),
        0
    );
}

#[tokio::test]
async fn automatic_migration_covers_fresh_partial_and_current_schemas() {
    let migration_count = Migrator::migrations().len();
    for applied_count in 0..=migration_count {
        let directory = tempfile::tempdir().expect("temporary migration database directory");
        let path = directory.path().join("startup.db");
        let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
            .await
            .expect("temporary file-backed database");

        if applied_count > 0 {
            Migrator::up(&db, Some(applied_count as u32))
                .await
                .expect("historical migration prefix should apply");
            db.execute(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO assets (ticker, name, asset_type, currency, created_at) VALUES ('XSTARTUP', 'Startup fixture', 'stock', 'EUR', '2025-01-01T00:00:00')",
            ))
            .await
            .expect("historical schema should accept persisted asset data");
            db.execute(Statement::from_string(
                DbBackend::Sqlite,
                "INSERT INTO transactions (asset_id, tx_type, date, quantity, price_cents, fees_cents, created_at) VALUES (1, 'buy', '2025-01-01', 1.0, 10000, 0, '2025-01-01T00:00:00')",
            ))
            .await
            .expect("historical schema should accept persisted ledger data");
        }

        assert_eq!(
            Migrator::get_pending_migrations(&db)
                .await
                .expect("historical migration state")
                .len(),
            migration_count - applied_count
        );
        rstock::db::migrate(&db)
            .await
            .expect("historical schema should migrate to current");
        assert!(Migrator::get_pending_migrations(&db)
            .await
            .expect("current migration state")
            .is_empty());

        if applied_count > 0 {
            let asset_count: i64 = db
                .query_one(Statement::from_string(
                    DbBackend::Sqlite,
                    "SELECT COUNT(*) AS count FROM assets WHERE ticker = 'XSTARTUP'",
                ))
                .await
                .expect("persisted asset query")
                .expect("persisted asset count row")
                .try_get("", "count")
                .expect("persisted asset count");
            let transaction_count: i64 = db
                .query_one(Statement::from_string(
                    DbBackend::Sqlite,
                    "SELECT COUNT(*) AS count FROM transactions WHERE asset_id = 1",
                ))
                .await
                .expect("persisted transaction query")
                .expect("persisted transaction count row")
                .try_get("", "count")
                .expect("persisted transaction count");
            assert_eq!((asset_count, transaction_count), (1, 1));
        }
    }
}

#[tokio::test]
async fn automatic_migration_rolls_back_a_failed_destructive_migration() {
    let directory = tempfile::tempdir().expect("temporary migration database directory");
    let path = directory.path().join("rollback.db");
    let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .expect("temporary file-backed database");
    Migrator::up(&db, Some((Migrator::migrations().len() - 1) as u32))
        .await
        .expect("historical migration prefix should apply");
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO daily_exchange_rates (from_currency, to_currency, date, rate) VALUES ('USD', 'EUR', '2025-01-01', 0.9)",
    ))
    .await
    .expect("historical exchange rate should be stored");
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "CREATE INDEX idx_transactions_date_id ON transactions (date, id)",
    ))
    .await
    .expect("conflicting index should be created");

    assert!(rstock::db::migrate(&db).await.is_err());
    assert_eq!(
        Migrator::get_pending_migrations(&db)
            .await
            .expect("migration state after rollback")
            .len(),
        1
    );
    let rate: f64 = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT rate FROM daily_exchange_rates WHERE from_currency = 'USD' AND to_currency = 'EUR'",
        ))
        .await
        .expect("rolled-back exchange rate query")
        .expect("rolled-back exchange rate row")
        .try_get("", "rate")
        .expect("rolled-back exchange rate");
    assert!((rate - 0.9).abs() < f64::EPSILON);
}
