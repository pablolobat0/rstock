use common::setup_test_db;
use migration::{Migrator, MigratorTrait};
use rstock::db::entities::transaction;
use sea_orm::{ConnectionTrait, DatabaseBackend, EntityTrait, QueryOrder, Set, Statement};

pub mod common;

#[tokio::test]
async fn semantic_migration_round_trips_every_transaction_kind_and_identity() {
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .expect("in-memory database");
    let prefix = (Migrator::migrations().len() - 1) as u32;
    Migrator::up(&db, Some(prefix))
        .await
        .expect("legacy migration prefix");
    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        "INSERT INTO assets (ticker, name, asset_type, currency, created_at) VALUES ('XSCHEMA', 'Schema fixture', 'stock', 'EUR', '2025-01-01T00:00:00')",
    ))
    .await
    .expect("asset");
    db.execute(Statement::from_string(
        DatabaseBackend::Sqlite,
        "INSERT INTO transactions (id, asset_id, tx_type, date, quantity, price_cents, fees_cents, created_at) VALUES
         (11, 1, 'buy', '2025-01-01', 10.5, 1234, 5, 'created-buy'),
         (12, 1, 'split', '2025-01-02', 2.0, 0, 0, 'created-split'),
         (13, 1, 'dividend', '2025-01-02', 1.0, 700, 100, 'created-dividend'),
         (14, 1, 'sell', '2025-01-03', 3.0, 1500, 7, 'created-sell')",
    ))
    .await
    .expect("legacy transactions");

    Migrator::up(&db, None).await.expect("semantic migration");
    let rows = transaction::Entity::find()
        .order_by_asc(transaction::Column::Id)
        .all(&db)
        .await
        .expect("semantic rows");
    assert_eq!(
        rows.iter().map(|row| row.id).collect::<Vec<_>>(),
        [11, 12, 13, 14]
    );
    assert_eq!(rows[0].units, Some(10.5));
    assert_eq!(rows[0].unit_price_cents, Some(1234));
    assert_eq!(rows[0].fees_cents, Some(5));
    assert_eq!(rows[1].split_ratio, Some(2.0));
    assert_eq!(rows[2].dividend_amount_cents, Some(700));
    assert_eq!(rows[2].dividend_deductions_cents, Some(100));
    assert_eq!(rows[3].units, Some(3.0));
    assert_eq!(rows[3].unit_price_cents, Some(1500));

    Migrator::down(&db, Some(1))
        .await
        .expect("legacy down migration");
    let legacy = db
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT id, tx_type, date, quantity, price_cents, fees_cents, created_at FROM transactions ORDER BY date, id",
        ))
        .await
        .expect("legacy rows");
    assert_eq!(legacy.len(), 4);
    assert_eq!(legacy[0].try_get::<i32>("", "id").unwrap(), 11);
    assert!((legacy[0].try_get::<f64>("", "quantity").unwrap() - 10.5).abs() < 1e-12);
    assert!((legacy[1].try_get::<f64>("", "quantity").unwrap() - 2.0).abs() < 1e-12);
    assert_eq!(legacy[1].try_get::<i64>("", "price_cents").unwrap(), 0);
    assert!((legacy[2].try_get::<f64>("", "quantity").unwrap() - 1.0).abs() < 1e-12);
    assert_eq!(legacy[2].try_get::<i64>("", "price_cents").unwrap(), 700);
    assert_eq!(legacy[2].try_get::<i64>("", "fees_cents").unwrap(), 100);
    assert_eq!(legacy[3].try_get::<i64>("", "price_cents").unwrap(), 1500);
    assert_eq!(
        legacy[3].try_get::<String>("", "created_at").unwrap(),
        "created-sell"
    );
}

#[tokio::test]
async fn semantic_constraints_reject_irrelevant_fields_and_invalid_signs() {
    let db = setup_test_db().await;
    let asset_id =
        common::insert_asset(&db, "XCONSTRAINT", "Constraint fixture", "stock", "EUR").await;
    let cases = [
        format!("INSERT INTO transactions (asset_id,tx_type,date,units,unit_price_cents,fees_cents) VALUES ({asset_id},'buy','2025-01-01',0,100,0)"),
        format!("INSERT INTO transactions (asset_id,tx_type,date,units,unit_price_cents,fees_cents) VALUES ({asset_id},'sell','2025-01-01',1,-1,0)"),
        format!("INSERT INTO transactions (asset_id,tx_type,date,units,unit_price_cents,fees_cents) VALUES ({asset_id},'buy', '2025-01-01',1.5,100.5,0)"),
        format!("INSERT INTO transactions (asset_id,tx_type,date,units,unit_price_cents,fees_cents) VALUES ({asset_id},'buy', '2025-01-01',1e999,100,0)"),
        format!("INSERT INTO transactions (asset_id,tx_type,date,dividend_amount_cents,dividend_deductions_cents) VALUES ({asset_id},'dividend','2025-01-01',10,11)"),
        format!("INSERT INTO transactions (asset_id,tx_type,date,split_ratio,fees_cents) VALUES ({asset_id},'split','2025-01-01',2,0)"),
        format!("INSERT INTO transactions (asset_id,tx_type,date,units,unit_price_cents,fees_cents) VALUES ({asset_id},'unknown','2025-01-01',1,100,0)"),
    ];
    for statement in cases {
        assert!(db
            .execute(Statement::from_string(DatabaseBackend::Sqlite, statement))
            .await
            .is_err());
    }

    let valid_buy = transaction::ActiveModel {
        asset_id: Set(asset_id),
        tx_type: Set("buy".to_owned()),
        date: Set("2025-01-01".to_owned()),
        units: Set(Some(1.0)),
        unit_price_cents: Set(Some(100)),
        fees_cents: Set(Some(0)),
        created_at: Set("now".to_owned()),
        ..Default::default()
    };
    transaction::Entity::insert(valid_buy)
        .exec(&db)
        .await
        .expect("valid semantic buy");
    for statement in [
        format!("INSERT INTO transactions (asset_id,tx_type,date,units,unit_price_cents,fees_cents,created_at) VALUES ({asset_id},'sell','2025-01-02',1,100,0,'now')"),
        format!("INSERT INTO transactions (asset_id,tx_type,date,dividend_amount_cents,dividend_deductions_cents,created_at) VALUES ({asset_id},'dividend','2025-01-02',10,0,'now')"),
        format!("INSERT INTO transactions (asset_id,tx_type,date,split_ratio,created_at) VALUES ({asset_id},'split','2025-01-02',2,'now')"),
    ] {
        db.execute(Statement::from_string(DatabaseBackend::Sqlite, statement))
            .await
            .expect("valid semantic transaction variant");
    }

    let indexes = db
        .query_all(Statement::from_string(
            DatabaseBackend::Sqlite,
            "PRAGMA index_list('transactions')",
        ))
        .await
        .expect("transaction indexes");
    let index_names = indexes
        .iter()
        .filter_map(|row| row.try_get::<String>("", "name").ok())
        .collect::<Vec<_>>();
    assert!(index_names
        .iter()
        .any(|name| name == "idx_transactions_date_id"));
    assert!(index_names
        .iter()
        .any(|name| name == "idx_transactions_asset_date_id"));
}
