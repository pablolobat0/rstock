//! Lightweight, deterministic checks supporting the offline performance gate.

mod common;

use common::{insert_asset, insert_transaction, setup_test_db};
use sea_orm::{ConnectionTrait, DbBackend, Statement};

#[tokio::test]
async fn representative_transaction_plans_are_available_for_baselining() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XPLAN1", "Plan fixture", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2020-01-01", 1.0, 100.0, 0.0).await;

    let queries = [
        "EXPLAIN QUERY PLAN SELECT * FROM transactions ORDER BY date ASC, id ASC",
        &format!("EXPLAIN QUERY PLAN SELECT * FROM transactions WHERE asset_id = {asset_id} ORDER BY date ASC, id ASC"),
        &format!("EXPLAIN QUERY PLAN SELECT * FROM transactions WHERE asset_id = {asset_id} AND date <= '2020-12-31' ORDER BY date ASC, id ASC"),
    ];
    let mut classified = Vec::new();
    for sql in queries {
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
        classified.push((
            details.iter().any(|detail| detail.contains("USING INDEX")),
            details.iter().any(|detail| detail.contains("TEMP B-TREE")),
        ));
    }
    assert_eq!(classified.len(), 3);
    // Baseline records whether indexes and temporary sorts are present.  The
    // optimization slice may later turn these observed scans into searches;
    // this foundation must not assume an index before that slice lands.
    assert_eq!(
        classified
            .iter()
            .filter(|(_, uses_temp_sort)| *uses_temp_sort)
            .count(),
        3
    );
}
