mod common;

use rstock::db::repos::transaction_repo;
use rstock::models::f64_to_cents;
use rstock::services;

use common::*;

#[tokio::test]
async fn test_find_by_id() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 1.0).await;

    let tx = transaction_repo::find_by_id(&db, 1).await.unwrap();
    assert!(tx.is_some());
    let tx = tx.unwrap();
    assert_eq!(tx.id, 1);
    assert_eq!(tx.asset_id, asset_id);
    assert_eq!(tx.quantity, 10.0);
}

#[tokio::test]
async fn test_find_by_id_not_found() {
    let db = setup_test_db().await;

    let tx = transaction_repo::find_by_id(&db, 999).await.unwrap();
    assert!(tx.is_none());
}

#[tokio::test]
async fn test_delete_by_id() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 1.0).await;

    transaction_repo::delete_by_id(&db, 1).await.unwrap();

    let tx = transaction_repo::find_by_id(&db, 1).await.unwrap();
    assert!(tx.is_none());
}

#[tokio::test]
async fn test_update_by_id_partial() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 1.0).await;

    // Update only price
    transaction_repo::update_by_id(&db, 1, None, None, Some(f64_to_cents(200.0)), None)
        .await
        .unwrap();

    let tx = transaction_repo::find_by_id(&db, 1).await.unwrap().unwrap();
    assert_eq!(tx.price_cents, f64_to_cents(200.0));
    assert_eq!(tx.quantity, 10.0); // unchanged
    assert_eq!(tx.date, "2025-01-02"); // unchanged
}

#[tokio::test]
async fn test_update_by_id_all_fields() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 1.0).await;

    transaction_repo::update_by_id(
        &db,
        1,
        Some("2025-02-01".to_owned()),
        Some(20.0),
        Some(f64_to_cents(150.0)),
        Some(f64_to_cents(2.0)),
    )
    .await
    .unwrap();

    let tx = transaction_repo::find_by_id(&db, 1).await.unwrap().unwrap();
    assert_eq!(tx.date, "2025-02-01");
    assert_eq!(tx.quantity, 20.0);
    assert_eq!(tx.price_cents, f64_to_cents(150.0));
    assert_eq!(tx.fees_cents, f64_to_cents(2.0));
}

#[tokio::test]
async fn test_insert_buy_returns_id() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;

    let order = rstock::models::BuyOrder {
        date: "2025-01-02".to_owned(),
        quantity: 10.0,
        price: 100.0,
        fees: 1.0,
    };
    let id = transaction_repo::insert_buy(&db, asset_id, &order)
        .await
        .unwrap();
    assert!(id > 0);
}

#[tokio::test]
async fn test_service_list_returns_identifying_transaction_details_in_date_order() {
    let db = setup_test_db().await;
    let asset_a = insert_asset(&db, "XFAKE1", "Fake Stock One", "stock", "EUR").await;
    let asset_b = insert_asset(&db, "XFAKE2", "Fake Stock Two", "stock", "EUR").await;
    insert_transaction(&db, asset_b, "2025-01-03", 3.0, 30.0, 0.0).await;
    insert_transaction(&db, asset_a, "2025-01-02", 2.0, 20.0, 1.0).await;

    let transactions = services::transactions::list(&db).await.unwrap();

    assert_eq!(transactions.len(), 2);
    assert_eq!(transactions[0].transaction.id, 2);
    assert_eq!(transactions[0].transaction.date, "2025-01-02");
    assert_eq!(transactions[0].ticker, "XFAKE1");
    assert_eq!(transactions[0].asset_name, "Fake Stock One");
    assert_eq!(transactions[1].transaction.id, 1);
    assert_eq!(transactions[1].transaction.date, "2025-01-03");
    assert_eq!(transactions[1].ticker, "XFAKE2");
}

#[tokio::test]
async fn test_service_delete_invalidates_snapshots() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;

    // Insert a portfolio snapshot after the transaction date
    insert_portfolio_snapshot(&db, "2025-01-03", 100.0, 10.0).await;

    let receipt = services::transactions::delete(&db, 1).await.unwrap();

    assert_eq!(receipt.transaction_id, 1);
    assert_eq!(receipt.summary, "Transaction 1 deleted.");

    // Snapshot should be invalidated
    let snapshots = get_all_snapshots(&db).await;
    assert!(snapshots.is_empty());
}

#[tokio::test]
async fn test_service_edit_invalidates_snapshots() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;

    // Insert a portfolio snapshot after the transaction date
    insert_portfolio_snapshot(&db, "2025-01-03", 100.0, 10.0).await;

    let receipt = services::transactions::edit(&db, 1, None, Some(20.0), None, None)
        .await
        .unwrap();

    assert_eq!(receipt.transaction_id, 1);
    assert_eq!(receipt.summary, "Transaction 1 updated.");

    // Snapshot should be invalidated
    let snapshots = get_all_snapshots(&db).await;
    assert!(snapshots.is_empty());
}
