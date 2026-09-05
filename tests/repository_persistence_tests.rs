mod common;

use sea_orm::{EntityTrait, QueryOrder, TransactionTrait};

use common::{insert_asset, setup_test_db};
use rstock::db::entities::portfolio_asset_history;
use rstock::db::repos::{
    asset_repo, portfolio_asset_history_repo, portfolio_history_repo, transaction_repo,
};
use rstock::models::{
    f64_to_cents, AssetClassification, AssetInfo, AssetSnapshot, AssetType, BuyOrder,
    DividendOrder, PortfolioSnapshot, SellOrder, SplitOrder,
};

#[tokio::test]
async fn repository_writes_commit_and_rollback_in_caller_owned_transactions() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;

    let rolled_back = db.begin().await.unwrap();
    asset_repo::create(
        &rolled_back,
        &asset_info("XFAKE2"),
        &AssetClassification::default(),
        None,
    )
    .await
    .unwrap();
    let rolled_back_id = transaction_repo::insert_buy(
        &rolled_back,
        asset_id,
        &buy_order("2025-01-02", 10.0, 12.345, 0.125),
    )
    .await
    .unwrap();
    assert!(transaction_repo::find_by_id(&rolled_back, rolled_back_id)
        .await
        .unwrap()
        .is_some());
    rolled_back.rollback().await.unwrap();

    assert!(transaction_repo::find_by_id(&db, rolled_back_id)
        .await
        .unwrap()
        .is_none());
    assert!(asset_repo::find_by_ticker(&db, "XFAKE2")
        .await
        .unwrap()
        .is_none());

    let committed = db.begin().await.unwrap();
    asset_repo::create(
        &committed,
        &asset_info("XFAKE3"),
        &AssetClassification::default(),
        None,
    )
    .await
    .unwrap();
    let committed_id = transaction_repo::insert_buy(
        &committed,
        asset_id,
        &buy_order("2025-01-03", 10.0, 12.345, 0.125),
    )
    .await
    .unwrap();
    committed.commit().await.unwrap();

    let committed_transaction = transaction_repo::find_by_id(&db, committed_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        committed_transaction.unit_price_cents,
        Some(f64_to_cents(12.345))
    );
    assert_eq!(
        committed_transaction.trade_fees_cents,
        Some(f64_to_cents(0.125))
    );
    assert!(asset_repo::find_by_ticker(&db, "XFAKE3")
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn complete_nav_snapshot_writes_share_one_transaction_boundary() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "USD").await;
    let portfolio = portfolio_snapshot("2025-01-02", 110.0);
    let assets = vec![asset_snapshot("2025-01-02", asset_id, 110.0)];

    let rolled_back = db.begin().await.unwrap();
    portfolio_history_repo::upsert_many(&rolled_back, &[portfolio_snapshot("2025-01-02", 110.0)])
        .await
        .unwrap();
    portfolio_asset_history_repo::upsert_many(&rolled_back, &assets)
        .await
        .unwrap();
    rolled_back.rollback().await.unwrap();

    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
    assert!(
        portfolio_asset_history_repo::find_by_date(&db, "2025-01-02")
            .await
            .unwrap()
            .is_empty()
    );

    let committed = db.begin().await.unwrap();
    portfolio_history_repo::upsert_many(&committed, &[portfolio])
        .await
        .unwrap();
    portfolio_asset_history_repo::upsert_many(&committed, &assets)
        .await
        .unwrap();
    committed.commit().await.unwrap();

    assert_eq!(
        portfolio_history_repo::find_latest(&db)
            .await
            .unwrap()
            .unwrap()
            .nav,
        110.0
    );
    assert_eq!(
        portfolio_asset_history_repo::find_by_date(&db, "2025-01-02")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn caller_transaction_rolls_back_completed_bulk_chunks_after_a_late_failure() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "EUR").await;
    let transaction = db.begin().await.unwrap();
    let mut writes = (0..100)
        .map(|_| transaction_repo::TransactionWrite::Buy {
            asset_id,
            order: buy_order("2025-01-02", 1.0, 10.015, 0.005),
        })
        .collect::<Vec<_>>();
    writes.push(transaction_repo::TransactionWrite::Buy {
        asset_id: i32::MAX,
        order: buy_order("2025-02-01", 1.0, 10.015, 0.005),
    });

    assert!(transaction_repo::insert_many(&transaction, &writes)
        .await
        .is_err());
    transaction.rollback().await.unwrap();

    assert!(transaction_repo::find_all_ordered_by_date(&db, None, None)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn create_or_find_by_ticker_reuses_concurrent_conflict_result() {
    let db = setup_test_db().await;
    let info = asset_info("XFAKE1");

    let first_id =
        asset_repo::create_or_find_by_ticker(&db, &info, &AssetClassification::default(), None)
            .await
            .unwrap();
    let second_id = asset_repo::create_or_find_by_ticker(
        &db,
        &AssetInfo {
            name: "Ignored Name".to_owned(),
            ..info
        },
        &AssetClassification::default(),
        None,
    )
    .await
    .unwrap();

    assert_eq!(second_id, first_id);
    assert_eq!(
        asset_repo::find_by_ticker(&db, "XFAKE1")
            .await
            .unwrap()
            .unwrap()
            .name,
        "Fake Stock"
    );
}

#[tokio::test]
async fn bulk_writes_match_single_row_writes_for_ledger_market_data_and_nav() {
    let single_db = setup_test_db().await;
    let bulk_db = setup_test_db().await;
    let single_asset = insert_asset(&single_db, "XFAKE1", "Fake Stock", "stock", "USD").await;
    let bulk_asset = insert_asset(&bulk_db, "XFAKE1", "Fake Stock", "stock", "USD").await;

    transaction_repo::insert_buy(
        &single_db,
        single_asset,
        &buy_order("2025-01-02", 2.0, 10.015, 0.125),
    )
    .await
    .unwrap();
    transaction_repo::insert_sell(
        &single_db,
        single_asset,
        &SellOrder {
            date: "2025-01-03".to_owned(),
            quantity: 0.5,
            price: 11.115,
            fees: 0.225,
        },
    )
    .await
    .unwrap();
    transaction_repo::insert_dividend(
        &single_db,
        single_asset,
        &DividendOrder {
            date: "2025-01-04".to_owned(),
            amount: 1.015,
            fees: 0.005,
        },
    )
    .await
    .unwrap();
    transaction_repo::insert_split(
        &single_db,
        single_asset,
        &SplitOrder {
            date: "2025-01-05".to_owned(),
            ratio: 2.0,
        },
    )
    .await
    .unwrap();

    let writes = vec![
        transaction_repo::TransactionWrite::Buy {
            asset_id: bulk_asset,
            order: buy_order("2025-01-02", 2.0, 10.015, 0.125),
        },
        transaction_repo::TransactionWrite::Sell {
            asset_id: bulk_asset,
            order: SellOrder {
                date: "2025-01-03".to_owned(),
                quantity: 0.5,
                price: 11.115,
                fees: 0.225,
            },
        },
        transaction_repo::TransactionWrite::Dividend {
            asset_id: bulk_asset,
            order: DividendOrder {
                date: "2025-01-04".to_owned(),
                amount: 1.015,
                fees: 0.005,
            },
        },
        transaction_repo::TransactionWrite::Split {
            asset_id: bulk_asset,
            order: SplitOrder {
                date: "2025-01-05".to_owned(),
                ratio: 2.0,
            },
        },
    ];
    transaction_repo::insert_many(&bulk_db, &writes)
        .await
        .unwrap();

    let single_transactions = transaction_repo::find_all_ordered_by_date(&single_db, None, None)
        .await
        .unwrap();
    let bulk_transactions = transaction_repo::find_all_ordered_by_date(&bulk_db, None, None)
        .await
        .unwrap();
    let ledger_fields = |transactions: Vec<rstock::models::Transaction>| {
        transactions
            .into_iter()
            .map(|tx| {
                (
                    tx.id,
                    tx.tx_type,
                    tx.date,
                    tx.units,
                    tx.unit_price_cents,
                    tx.trade_fees_cents,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ledger_fields(single_transactions),
        ledger_fields(bulk_transactions)
    );

    let portfolio_snapshots = vec![
        portfolio_snapshot("2025-01-02", 100.0),
        portfolio_snapshot("2025-01-03", 101.0),
    ];
    portfolio_history_repo::upsert_many(&single_db, &portfolio_snapshots)
        .await
        .unwrap();
    portfolio_history_repo::upsert_many(&bulk_db, &portfolio_snapshots)
        .await
        .unwrap();

    let single_nav = portfolio_history_repo::find_between(&single_db, "2025-01-01", "2025-01-04")
        .await
        .unwrap();
    let bulk_nav = portfolio_history_repo::find_between(&bulk_db, "2025-01-01", "2025-01-04")
        .await
        .unwrap();
    assert_eq!(snapshot_values(&single_nav), snapshot_values(&bulk_nav));

    let single_asset_snapshots = vec![
        asset_snapshot("2025-01-02", single_asset, 10.0),
        asset_snapshot("2025-01-03", single_asset, 11.0),
    ];
    portfolio_asset_history_repo::upsert_many(&single_db, &single_asset_snapshots)
        .await
        .unwrap();
    let bulk_asset_snapshots = vec![
        asset_snapshot("2025-01-02", bulk_asset, 10.0),
        asset_snapshot("2025-01-03", bulk_asset, 11.0),
    ];
    portfolio_asset_history_repo::upsert_many(&bulk_db, &bulk_asset_snapshots)
        .await
        .unwrap();

    let single_rows = portfolio_asset_history::Entity::find()
        .order_by_asc(portfolio_asset_history::Column::Date)
        .all(&single_db)
        .await
        .unwrap();
    let bulk_rows = portfolio_asset_history::Entity::find()
        .order_by_asc(portfolio_asset_history::Column::Date)
        .all(&bulk_db)
        .await
        .unwrap();
    let asset_values = |rows: Vec<portfolio_asset_history::Model>| {
        rows.into_iter()
            .map(|row| {
                (
                    row.date,
                    row.quantity,
                    row.closing_price,
                    row.market_value,
                    row.exchange_rate,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(asset_values(single_rows), asset_values(bulk_rows));
}

fn buy_order(date: &str, quantity: f64, price: f64, fees: f64) -> BuyOrder {
    BuyOrder {
        date: date.to_owned(),
        quantity,
        price,
        fees,
    }
}

fn asset_info(ticker: &str) -> AssetInfo {
    AssetInfo {
        ticker: ticker.to_owned(),
        name: "Fake Stock".to_owned(),
        asset_type: AssetType::Stock,
        currency: "EUR".to_owned(),
    }
}

fn portfolio_snapshot(date: &str, nav: f64) -> PortfolioSnapshot {
    PortfolioSnapshot {
        date: date.to_owned(),
        asset_value: nav,
        total_value: nav,
        outstanding_shares: 1.0,
        nav,
    }
}

fn asset_snapshot(date: &str, asset_id: i32, value: f64) -> AssetSnapshot {
    AssetSnapshot {
        date: date.to_owned(),
        asset_id,
        quantity: 1.0,
        closing_price: value / 0.9,
        market_value: value,
        exchange_rate: 0.9,
    }
}

fn snapshot_values(snapshots: &[PortfolioSnapshot]) -> Vec<(&str, f64, f64, f64, f64)> {
    snapshots
        .iter()
        .map(|snapshot| {
            (
                snapshot.date.as_str(),
                snapshot.asset_value,
                snapshot.total_value,
                snapshot.outstanding_shares,
                snapshot.nav,
            )
        })
        .collect()
}
