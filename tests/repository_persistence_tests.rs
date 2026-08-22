mod common;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, TransactionTrait};

use common::{insert_asset, setup_test_db};
use rstock::db::entities::{daily_asset_price, daily_exchange_rate, portfolio_asset_history};
use rstock::db::repos::{
    asset_repo, daily_price_repo, exchange_rate_repo, portfolio_asset_history_repo,
    portfolio_history_repo, transaction_repo,
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
    daily_price_repo::upsert(&rolled_back, asset_id, "2025-01-02", 12.5, false)
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
    assert_eq!(
        daily_price_repo::find_price(&db, asset_id, "2025-01-02")
            .await
            .unwrap(),
        None
    );
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
    daily_price_repo::upsert(&committed, asset_id, "2025-01-03", 12.5, false)
        .await
        .unwrap();
    committed.commit().await.unwrap();

    let committed_transaction = transaction_repo::find_by_id(&db, committed_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(committed_transaction.price_cents, f64_to_cents(12.345));
    assert_eq!(committed_transaction.fees_cents, f64_to_cents(0.125));
    assert_eq!(
        daily_price_repo::find_price(&db, asset_id, "2025-01-03")
            .await
            .unwrap(),
        Some(12.5)
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
    portfolio_history_repo::upsert(&rolled_back, &portfolio)
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
    portfolio_history_repo::upsert(&committed, &portfolio)
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
async fn native_conflicts_update_existing_values_without_manual_reads() {
    let db = setup_test_db().await;
    let asset_id = insert_asset(&db, "XFAKE1", "Fake Stock", "stock", "USD").await;

    daily_price_repo::upsert(&db, asset_id, "2025-01-02", 10.0, true)
        .await
        .unwrap();
    let original_price = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    daily_price_repo::upsert(&db, asset_id, "2025-01-02", 11.5, false)
        .await
        .unwrap();
    let updated_price = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_price.id, original_price.id);
    assert_eq!(updated_price.closing_price, 11.5);
    assert!(!updated_price.is_api_failure);

    exchange_rate_repo::upsert(&db, "USD", "EUR", "2025-01-02", 0.90)
        .await
        .unwrap();
    let original_rate = daily_exchange_rate::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    exchange_rate_repo::upsert(&db, "USD", "EUR", "2025-01-02", 0.91)
        .await
        .unwrap();
    let updated_rate = daily_exchange_rate::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_rate.id, original_rate.id);
    assert_eq!(updated_rate.rate, 0.91);

    portfolio_history_repo::upsert(&db, &portfolio_snapshot("2025-01-02", 100.0))
        .await
        .unwrap();
    portfolio_history_repo::upsert(&db, &portfolio_snapshot("2025-01-02", 101.0))
        .await
        .unwrap();
    let updated_portfolio = portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated_portfolio.date, "2025-01-02");
    assert_eq!(updated_portfolio.nav, 101.0);

    let first_snapshot = asset_snapshot("2025-01-02", asset_id, 10.0);
    portfolio_asset_history_repo::upsert(&db, &first_snapshot)
        .await
        .unwrap();
    let original_snapshot = portfolio_asset_history::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    let updated_snapshot = asset_snapshot("2025-01-02", asset_id, 12.0);
    portfolio_asset_history_repo::upsert(&db, &updated_snapshot)
        .await
        .unwrap();
    let persisted_snapshot = portfolio_asset_history::Entity::find()
        .one(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(persisted_snapshot.id, original_snapshot.id);
    assert_eq!(persisted_snapshot.market_value, 12.0);
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
                    tx.quantity,
                    tx.price_cents,
                    tx.fees_cents,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        ledger_fields(single_transactions),
        ledger_fields(bulk_transactions)
    );

    daily_price_repo::upsert(&single_db, single_asset, "2025-01-02", 10.0, true)
        .await
        .unwrap();
    daily_price_repo::upsert(&single_db, single_asset, "2025-01-02", 11.0, false)
        .await
        .unwrap();
    daily_price_repo::upsert(&single_db, single_asset, "2025-01-03", 12.0, false)
        .await
        .unwrap();
    daily_price_repo::upsert_many(
        &bulk_db,
        &[
            daily_price_repo::DailyPriceWrite {
                asset_id: bulk_asset,
                date: "2025-01-02".to_owned(),
                price: 10.0,
                is_api_failure: true,
            },
            daily_price_repo::DailyPriceWrite {
                asset_id: bulk_asset,
                date: "2025-01-02".to_owned(),
                price: 11.0,
                is_api_failure: false,
            },
            daily_price_repo::DailyPriceWrite {
                asset_id: bulk_asset,
                date: "2025-01-03".to_owned(),
                price: 12.0,
                is_api_failure: false,
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        daily_price_repo::find_prices_between(&single_db, single_asset, "2025-01-01", "2025-01-04")
            .await
            .unwrap(),
        daily_price_repo::find_prices_between(&bulk_db, bulk_asset, "2025-01-01", "2025-01-04")
            .await
            .unwrap()
    );

    exchange_rate_repo::upsert(&single_db, "USD", "EUR", "2025-01-02", 0.90)
        .await
        .unwrap();
    exchange_rate_repo::upsert(&single_db, "USD", "EUR", "2025-01-02", 0.91)
        .await
        .unwrap();
    exchange_rate_repo::upsert(&single_db, "USD", "EUR", "2025-01-03", 0.92)
        .await
        .unwrap();
    exchange_rate_repo::upsert_many(
        &bulk_db,
        &[
            exchange_rate_repo::ExchangeRateWrite {
                from_currency: "USD".to_owned(),
                to_currency: "EUR".to_owned(),
                date: "2025-01-02".to_owned(),
                rate: 0.90,
            },
            exchange_rate_repo::ExchangeRateWrite {
                from_currency: "USD".to_owned(),
                to_currency: "EUR".to_owned(),
                date: "2025-01-02".to_owned(),
                rate: 0.91,
            },
            exchange_rate_repo::ExchangeRateWrite {
                from_currency: "USD".to_owned(),
                to_currency: "EUR".to_owned(),
                date: "2025-01-03".to_owned(),
                rate: 0.92,
            },
        ],
    )
    .await
    .unwrap();
    assert_eq!(
        exchange_rate_repo::find_rates_between(
            &single_db,
            "USD",
            "EUR",
            "2025-01-01",
            "2025-01-04"
        )
        .await
        .unwrap(),
        exchange_rate_repo::find_rates_between(&bulk_db, "USD", "EUR", "2025-01-01", "2025-01-04")
            .await
            .unwrap()
    );

    let portfolio_snapshots = vec![
        portfolio_snapshot("2025-01-02", 100.0),
        portfolio_snapshot("2025-01-03", 101.0),
    ];
    for snapshot in &portfolio_snapshots {
        portfolio_history_repo::upsert(&single_db, snapshot)
            .await
            .unwrap();
    }
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
    for snapshot in &single_asset_snapshots {
        portfolio_asset_history_repo::upsert(&single_db, snapshot)
            .await
            .unwrap();
    }
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

    let bulk_rates = daily_exchange_rate::Entity::find()
        .all(&bulk_db)
        .await
        .unwrap();
    assert_eq!(bulk_rates.len(), 2);
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
