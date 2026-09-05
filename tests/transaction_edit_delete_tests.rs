mod common;

use rstock::db::repos::transaction_repo;
use rstock::models::{f64_to_cents, BuyOrder, DividendOrder, SellOrder, SplitOrder};
use rstock::services::{self, ledger};
use sea_orm::ConnectionTrait;

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

#[derive(Clone, Copy, Debug)]
enum MutationCase {
    Buy,
    Sell,
    Dividend,
    Split,
    Edit,
    Delete,
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn ledger_mutations_roll_back_when_snapshot_invalidation_fails() {
    for case in [
        MutationCase::Buy,
        MutationCase::Sell,
        MutationCase::Dividend,
        MutationCase::Split,
        MutationCase::Edit,
        MutationCase::Delete,
    ] {
        let db = setup_test_db().await;
        let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
        if !matches!(case, MutationCase::Buy) {
            insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
        }
        insert_portfolio_snapshot(&db, "2025-01-03", 100.0, 10.0).await;
        insert_portfolio_asset_snapshot(&db, "2025-01-03", asset_id, 10.0, 100.0, 1000.0, 1.0)
            .await;
        if matches!(case, MutationCase::Split) {
            insert_daily_price(&db, asset_id, "2025-01-01", 100.0, false).await;
        }

        let before = ledger_fields(
            transaction_repo::find_by_asset_id(&db, asset_id)
                .await
                .unwrap(),
        );
        db.execute_unprepared(
            "CREATE TRIGGER fail_snapshot_invalidation BEFORE DELETE ON portfolio_history \
             BEGIN SELECT RAISE(ABORT, 'injected invalidation failure'); END",
        )
        .await
        .unwrap();

        let result = match case {
            MutationCase::Buy => {
                services::transactions::buy(
                    &db,
                    "XFAKE1".to_owned(),
                    BuyOrder {
                        date: "2025-01-02".to_owned(),
                        quantity: 1.0,
                        price: 110.0,
                        fees: 1.0,
                    },
                )
                .await
            }
            MutationCase::Sell => {
                services::transactions::sell(
                    &db,
                    "XFAKE1".to_owned(),
                    SellOrder {
                        date: "2025-01-02".to_owned(),
                        quantity: 1.0,
                        price: 110.0,
                        fees: 1.0,
                    },
                )
                .await
            }
            MutationCase::Dividend => {
                services::transactions::dividend(
                    &db,
                    "XFAKE1".to_owned(),
                    DividendOrder {
                        date: "2025-01-02".to_owned(),
                        amount: 5.0,
                        fees: 1.0,
                    },
                )
                .await
            }
            MutationCase::Split => {
                services::transactions::split(
                    &db,
                    "XFAKE1".to_owned(),
                    SplitOrder {
                        date: "2025-01-02".to_owned(),
                        ratio: 2.0,
                    },
                )
                .await
            }
            MutationCase::Edit => {
                services::transactions::edit(&db, 1, None, Some(20.0), None, None).await
            }
            MutationCase::Delete => services::transactions::delete(&db, 1).await,
        };

        assert!(
            result.is_err(),
            "{case:?} should surface invalidation failure"
        );
        assert_eq!(
            ledger_fields(
                transaction_repo::find_by_asset_id(&db, asset_id)
                    .await
                    .unwrap()
            ),
            before,
            "{case:?} ledger change should roll back"
        );
        assert_eq!(get_all_snapshots(&db).await.len(), 1);
        assert_eq!(get_asset_snapshots(&db, "2025-01-03").await.len(), 1);
        if matches!(case, MutationCase::Split) {
            assert_eq!(
                common::find_daily_price(&db, asset_id, "2025-01-01")
                    .await
                    .unwrap(),
                Some(100.0_f64),
                "split price-cache invalidation should roll back"
            );
        }
    }
}

#[tokio::test]
async fn split_edit_and_delete_roll_back_when_price_cache_invalidation_fails() {
    for delete_split in [false, true] {
        let db = setup_test_db().await;
        let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
        insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
        insert_split_transaction(&db, asset_id, "2025-01-02", 2.0).await;
        insert_daily_price(&db, asset_id, "2025-01-01", 100.0, false).await;
        insert_portfolio_snapshot(&db, "2025-01-03", 100.0, 10.0).await;
        let before = ledger_fields(
            transaction_repo::find_by_asset_id(&db, asset_id)
                .await
                .unwrap(),
        );
        db.execute_unprepared(
            "CREATE TRIGGER fail_price_invalidation BEFORE DELETE ON daily_asset_prices \
             BEGIN SELECT RAISE(ABORT, 'injected price invalidation failure'); END",
        )
        .await
        .unwrap();

        let result = if delete_split {
            services::transactions::delete(&db, 2).await
        } else {
            services::transactions::edit(&db, 2, None, Some(3.0), None, None).await
        };

        assert!(result.is_err());
        assert_eq!(
            ledger_fields(
                transaction_repo::find_by_asset_id(&db, asset_id)
                    .await
                    .unwrap()
            ),
            before
        );
        assert_eq!(get_all_snapshots(&db).await.len(), 1);
        let cached_price = common::find_daily_price(&db, asset_id, "2025-01-01")
            .await
            .unwrap()
            .expect("split price should remain after rollback");
        assert!((cached_price - 100.0).abs() < f64::EPSILON);
    }
}

#[tokio::test]
async fn split_edit_invalidates_from_asset_earliest_transaction_after_date_move() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_split_transaction(&db, asset_id, "2025-01-02", 2.0).await;
    insert_daily_price(&db, asset_id, "2025-01-01", 100.0, false).await;
    insert_portfolio_snapshot(&db, "2025-01-01", 100.0, 10.0).await;
    insert_portfolio_snapshot(&db, "2025-01-03", 100.0, 10.0).await;

    services::transactions::edit(&db, 2, Some("2025-01-04".to_owned()), Some(3.0), None, None)
        .await
        .unwrap();

    assert!(get_all_snapshots(&db).await.is_empty());
    assert!(common::find_daily_price(&db, asset_id, "2025-01-01")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn deleting_split_invalidates_from_asset_earliest_transaction() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_split_transaction(&db, asset_id, "2025-01-02", 2.0).await;
    insert_daily_price(&db, asset_id, "2025-01-01", 100.0, false).await;
    insert_portfolio_snapshot(&db, "2025-01-01", 100.0, 10.0).await;
    insert_portfolio_snapshot(&db, "2025-01-03", 100.0, 10.0).await;

    services::transactions::delete(&db, 2).await.unwrap();

    assert!(get_all_snapshots(&db).await.is_empty());
    assert!(common::find_daily_price(&db, asset_id, "2025-01-01")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn mutation_invalidates_complete_snapshots_for_every_asset() {
    let db = setup_test_db().await;
    let changed_asset = insert_asset(&db, "XFAKE1", "Fake Stock One", "stock", "EUR").await;
    let other_asset = insert_asset(&db, "XFAKE2", "Fake Stock Two", "stock", "EUR").await;
    insert_transaction(&db, changed_asset, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_portfolio_snapshot(&db, "2025-01-03", 100.0, 10.0).await;
    for asset_id in [changed_asset, other_asset] {
        insert_portfolio_asset_snapshot(&db, "2025-01-03", asset_id, 10.0, 100.0, 1000.0, 1.0)
            .await;
    }

    services::transactions::edit(&db, 1, None, Some(20.0), None, None)
        .await
        .unwrap();

    assert!(get_all_snapshots(&db).await.is_empty());
    assert!(get_asset_snapshots(&db, "2025-01-03").await.is_empty());
}

#[tokio::test]
async fn same_day_ledger_reads_use_ascending_transaction_id() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 1.0, 100.0, 0.0).await;
    insert_split_transaction(&db, asset_id, "2025-01-02", 2.0).await;
    insert_transaction(&db, asset_id, "2025-01-02", 3.0, 100.0, 0.0).await;

    let per_asset = transaction_repo::find_by_asset_id(&db, asset_id)
        .await
        .unwrap();
    let all = transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .unwrap();

    assert_eq!(
        per_asset.iter().map(|tx| tx.id).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(
        all.iter().map(|tx| tx.id).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(
        ledger::replay_transactions(asset_id, &per_asset)
            .unwrap()
            .final_quantity,
        5.0
    );
}

#[tokio::test]
async fn recording_commands_validate_shapes_before_persistence() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;

    let invalid_results = [
        services::transactions::buy(
            &db,
            "XFAKE1".to_owned(),
            BuyOrder {
                date: "not-a-date".to_owned(),
                quantity: 1.0,
                price: 10.0,
                fees: 0.0,
            },
        )
        .await,
        services::transactions::sell(
            &db,
            "XFAKE1".to_owned(),
            SellOrder {
                date: "2025-01-01".to_owned(),
                quantity: f64::NAN,
                price: 10.0,
                fees: 0.0,
            },
        )
        .await,
        services::transactions::dividend(
            &db,
            "XFAKE1".to_owned(),
            DividendOrder {
                date: "2025-01-01".to_owned(),
                amount: 1.0,
                fees: 2.0,
            },
        )
        .await,
        services::transactions::split(
            &db,
            "XFAKE1".to_owned(),
            SplitOrder {
                date: "2025-01-01".to_owned(),
                ratio: f64::INFINITY,
            },
        )
        .await,
    ];

    assert!(invalid_results.into_iter().all(|result| result.is_err()));
    assert!(transaction_repo::find_by_asset_id(&db, asset_id)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn recording_replays_the_complete_ledger_and_rolls_back_invalid_suffixes() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 1.0, 100.0, 0.0).await;
    insert_sell_transaction(&db, asset_id, "2025-01-03", 1.0, 100.0, 0.0).await;
    let before = transaction_repo::find_by_asset_id(&db, asset_id)
        .await
        .unwrap();

    let result = services::transactions::sell(
        &db,
        "XFAKE1".to_owned(),
        SellOrder {
            date: "2025-01-02".to_owned(),
            quantity: 1.0,
            price: 100.0,
            fees: 0.0,
        },
    )
    .await;

    assert!(result.is_err());
    assert_eq!(
        transaction_repo::find_by_asset_id(&db, asset_id)
            .await
            .unwrap()
            .iter()
            .map(|tx| tx.id)
            .collect::<Vec<_>>(),
        before.iter().map(|tx| tx.id).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn same_day_recording_uses_generated_id_order_for_split_effects() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 1.0, 100.0, 0.0).await;

    let receipt = services::transactions::split(
        &db,
        "XFAKE1".to_owned(),
        SplitOrder {
            date: "2025-01-01".to_owned(),
            ratio: 2.0,
        },
    )
    .await
    .unwrap();

    assert_eq!(receipt.transaction_id, 2);
    let entries = transaction_repo::find_by_asset_id(&db, asset_id)
        .await
        .unwrap();
    let replay = rstock::services::ledger::CanonicalLedger::new(
        asset_id,
        entries
            .iter()
            .map(|tx| rstock::services::ledger::LedgerEntry {
                id: tx.id,
                asset_id: tx.asset_id,
                date: tx.date.clone(),
                kind: match &tx.tx_type {
                    rstock::models::TxType::Buy => rstock::services::ledger::LedgerEntryKind::Buy {
                        units: tx.quantity,
                        unit_price_cents: tx.price_cents,
                        fees_cents: tx.fees_cents,
                    },
                    rstock::models::TxType::Split => {
                        rstock::services::ledger::LedgerEntryKind::Split { ratio: tx.quantity }
                    }
                    _ => unreachable!("test ledger only contains buy and split"),
                },
            })
            .collect(),
    )
    .unwrap()
    .replay()
    .unwrap();
    assert!((replay.final_quantity - 2.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn editing_a_buy_replays_and_rejects_an_invalid_later_sell_atomically() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 1.0).await;
    insert_sell_transaction(&db, asset_id, "2025-01-03", 10.0, 100.0, 0.0).await;

    let result = services::transactions::edit(&db, 1, None, Some(9.0), None, None).await;

    assert!(result.is_err());
    let transactions = transaction_repo::find_by_asset_id(&db, asset_id)
        .await
        .unwrap();
    assert_eq!(transactions[0].quantity, 10.0);
    assert_eq!(transactions[0].id, 1);
    assert_eq!(transactions[1].id, 2);
}

#[tokio::test]
async fn editing_a_sell_replays_and_rejects_an_oversell_atomically() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_sell_transaction(&db, asset_id, "2025-01-02", 5.0, 100.0, 0.0).await;

    let result = services::transactions::edit(&db, 2, None, Some(11.0), None, None).await;

    assert!(result.is_err());
    assert_eq!(
        transaction_repo::find_by_id(&db, 2)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        5.0
    );
}

#[tokio::test]
async fn moving_a_sell_before_its_buy_is_rejected_without_reordering_identity() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_sell_transaction(&db, asset_id, "2025-01-03", 1.0, 100.0, 0.0).await;

    let result =
        services::transactions::edit(&db, 2, Some("2025-01-01".to_owned()), None, None, None).await;

    assert!(result.is_err());
    let transaction = transaction_repo::find_by_id(&db, 2).await.unwrap().unwrap();
    assert_eq!(transaction.date, "2025-01-03");
}

#[tokio::test]
async fn deleting_an_acquisition_required_by_later_entries_is_atomic() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_sell_transaction(&db, asset_id, "2025-01-02", 1.0, 100.0, 0.0).await;

    let result = services::transactions::delete(&db, 1).await;

    assert!(result.is_err());
    assert_eq!(
        transaction_repo::find_by_asset_id(&db, asset_id)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn changing_a_split_that_supports_a_later_sell_is_rejected_atomically() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_split_transaction(&db, asset_id, "2025-01-02", 2.0).await;
    insert_sell_transaction(&db, asset_id, "2025-01-03", 19.0, 100.0, 0.0).await;

    let result = services::transactions::edit(&db, 2, None, Some(1.0), None, None).await;

    assert!(result.is_err());
    assert_eq!(
        transaction_repo::find_by_id(&db, 2)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        2.0
    );
}

#[tokio::test]
async fn deleting_a_split_required_by_a_later_sell_is_atomic() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_split_transaction(&db, asset_id, "2025-01-02", 2.0).await;
    insert_sell_transaction(&db, asset_id, "2025-01-03", 19.0, 100.0, 0.0).await;

    let result = services::transactions::delete(&db, 2).await;

    assert!(result.is_err());
    assert_eq!(
        transaction_repo::find_by_asset_id(&db, asset_id)
            .await
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test]
async fn edits_reject_fields_that_are_not_meaningful_for_the_transaction_type() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_dividend_transaction(&db, asset_id, "2025-01-02", 5.0, 1.0).await;
    insert_split_transaction(&db, asset_id, "2025-01-03", 2.0).await;

    let dividend_result = services::transactions::edit(&db, 2, None, Some(2.0), None, None).await;
    let split_result = services::transactions::edit(&db, 3, None, None, Some(100.0), None).await;
    let split_fee_result = services::transactions::edit(&db, 3, None, None, None, Some(1.0)).await;

    assert!(dividend_result.is_err());
    assert!(split_result.is_err());
    assert!(split_fee_result.is_err());
    assert_eq!(
        transaction_repo::find_by_id(&db, 2)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        1.0
    );
    assert_eq!(
        transaction_repo::find_by_id(&db, 3)
            .await
            .unwrap()
            .unwrap()
            .quantity,
        2.0
    );
}

#[tokio::test]
async fn editing_preserves_id_and_uses_id_to_break_same_day_date_ties() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    insert_transaction(&db, asset_id, "2025-01-01", 10.0, 100.0, 0.0).await;
    insert_transaction(&db, asset_id, "2025-01-03", 2.0, 100.0, 0.0).await;
    insert_transaction(&db, asset_id, "2025-01-02", 3.0, 100.0, 0.0).await;

    services::transactions::edit(&db, 2, Some("2025-01-02".to_owned()), None, None, None)
        .await
        .unwrap();

    let transactions = transaction_repo::find_by_asset_id(&db, asset_id)
        .await
        .unwrap();
    assert_eq!(
        transactions
            .iter()
            .map(|transaction| transaction.id)
            .collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    assert_eq!(transactions[1].date, "2025-01-02");
    assert_eq!(transactions[2].date, "2025-01-02");
}

fn ledger_fields(
    transactions: Vec<rstock::models::Transaction>,
) -> Vec<(i32, String, String, f64, i64, i64)> {
    transactions
        .into_iter()
        .map(|tx| {
            (
                tx.id,
                tx.tx_type.to_string(),
                tx.date,
                tx.quantity,
                tx.price_cents,
                tx.fees_cents,
            )
        })
        .collect()
}
