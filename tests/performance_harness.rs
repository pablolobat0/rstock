//! Lightweight, deterministic checks supporting the offline performance gate.

mod common;

use common::{insert_asset, insert_transaction, setup_test_db};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

#[tokio::test]
async fn representative_transaction_plans_are_available_for_baselining() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XPLAN1", "Plan fixture", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2020-01-01", 1.0, 100.0, 0.0).await;

    for sql in [
        "EXPLAIN QUERY PLAN SELECT * FROM transactions ORDER BY date ASC, id ASC",
        "EXPLAIN QUERY PLAN SELECT * FROM transactions WHERE asset_id = 1 ORDER BY date ASC, id ASC",
        "EXPLAIN QUERY PLAN SELECT * FROM transactions WHERE asset_id = 1 AND date <= '2020-12-31' ORDER BY date ASC, id ASC",
    ] {
        let rows = db
            .query_all(Statement::from_string(DbBackend::Sqlite, sql))
            .await
            .expect("SQLite should produce a query plan");
        assert!(!rows.is_empty(), "query plan must contain at least one step");
    }
}
