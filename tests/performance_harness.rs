//! Lightweight, deterministic checks supporting the offline performance gate.

mod common;

use common::{insert_asset, insert_transaction, setup_test_db};
use rstock::db::repos::transaction_repo;
use sea_orm::{ConnectionTrait, DbBackend, Statement};

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
        let access = if details.iter().any(|detail| detail.contains("SEARCH")) {
            "search"
        } else {
            "scan"
        };
        let uses_index = details.iter().any(|detail| detail.contains("USING INDEX"));
        let uses_temp_sort = details.iter().any(|detail| detail.contains("TEMP B-TREE"));
        classified.push((access, uses_index, uses_temp_sort));
    }
    assert_eq!(classified.len(), 5);
    println!("transaction_query_plans={classified:?}");
    // Baseline records whether indexes and temporary sorts are present.  The
    // optimization slice may later turn these observed scans into searches;
    // this foundation must not assume an index before that slice lands.
    assert_eq!(
        classified
            .iter()
            .filter(|(_, _, uses_temp_sort)| *uses_temp_sort)
            .count(),
        3
    );
}
