mod common;

use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use rstock::db::entities::asset;
use rstock::db::repos::asset_repo;
use rstock::models::{
    AssetClass, AssetClassification, AssetInfo, AssetType, BondCredit, BondDuration, BuyOrder,
    EquityStyle, Management,
};
use rstock::services::transactions;

fn full_classification() -> AssetClassification {
    AssetClassification {
        asset_class: Some(AssetClass::Equity),
        equity_style: Some(EquityStyle::Blend),
        bond_credit: Some(BondCredit::Government),
        bond_duration: Some(BondDuration::Long),
        management: Some(Management::Passive),
    }
}

fn equity_info() -> AssetInfo {
    AssetInfo {
        ticker: "XFAKE1".to_owned(),
        name: "Fake ETF".to_owned(),
        asset_type: AssetType::Etf,
        currency: "EUR".to_owned(),
    }
}

#[tokio::test]
async fn create_roundtrips_all_classification_fields() {
    let db = common::setup_test_db().await;
    let info = equity_info();
    let classification = full_classification();

    let id = asset_repo::create(&db, &info, &classification, Some("MSTAR123"))
        .await
        .unwrap();

    let row = asset::Entity::find_by_id(id)
        .one(&db)
        .await
        .unwrap()
        .expect("asset should exist");

    assert_eq!(row.ticker, "XFAKE1");
    assert_eq!(row.name, "Fake ETF");
    assert_eq!(row.asset_type, "etf");
    assert_eq!(row.currency, "EUR");
    assert_eq!(row.morningstar_code.as_deref(), Some("MSTAR123"));
    assert_eq!(row.asset_class.as_deref(), Some("equity"));
    assert_eq!(row.equity_style.as_deref(), Some("blend"));
    assert_eq!(row.bond_credit.as_deref(), Some("government"));
    assert_eq!(row.bond_duration.as_deref(), Some("long"));
    assert_eq!(row.management.as_deref(), Some("passive"));
}

#[tokio::test]
async fn create_with_no_classification_stores_nulls() {
    let db = common::setup_test_db().await;
    let info = equity_info();

    let id = asset_repo::create(&db, &info, &AssetClassification::default(), None)
        .await
        .unwrap();

    let row = asset::Entity::find_by_id(id)
        .one(&db)
        .await
        .unwrap()
        .expect("asset should exist");

    assert!(row.asset_class.is_none());
    assert!(row.equity_style.is_none());
    assert!(row.bond_credit.is_none());
    assert!(row.bond_duration.is_none());
    assert!(row.management.is_none());
    assert!(row.morningstar_code.is_none());
}

#[tokio::test]
async fn create_errors_on_duplicate_ticker() {
    let db = common::setup_test_db().await;
    let info = equity_info();
    let classification = AssetClassification::default();

    asset_repo::create(&db, &info, &classification, None)
        .await
        .unwrap();

    let err = asset_repo::create(&db, &info, &classification, None)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("already exists"),
        "expected 'already exists' error, got: {err}"
    );
}

#[tokio::test]
async fn update_only_touches_provided_fields() {
    let db = common::setup_test_db().await;
    let info = equity_info();
    let classification = full_classification();

    asset_repo::create(&db, &info, &classification, Some("MSTAR123"))
        .await
        .unwrap();

    // Update only management and name, leave everything else untouched
    let partial = AssetClassification {
        management: Some(Management::Active),
        ..Default::default()
    };
    asset_repo::update(&db, "XFAKE1", &partial, Some("New Name"), None)
        .await
        .unwrap();

    let row = asset::Entity::find()
        .filter(asset::Column::Ticker.eq("XFAKE1"))
        .one(&db)
        .await
        .unwrap()
        .expect("asset should exist");

    // Changed fields
    assert_eq!(row.name, "New Name");
    assert_eq!(row.management.as_deref(), Some("active"));
    // Unchanged fields
    assert_eq!(row.asset_class.as_deref(), Some("equity"));
    assert_eq!(row.equity_style.as_deref(), Some("blend"));
    assert_eq!(row.bond_credit.as_deref(), Some("government"));
    assert_eq!(row.bond_duration.as_deref(), Some("long"));
    assert_eq!(row.morningstar_code.as_deref(), Some("MSTAR123"));
}

#[tokio::test]
async fn update_errors_on_missing_ticker() {
    let db = common::setup_test_db().await;

    let err = asset_repo::update(
        &db,
        "NONEXISTENT",
        &AssetClassification::default(),
        None,
        None,
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("not found"),
        "expected 'not found' error, got: {err}"
    );
}

#[tokio::test]
async fn buy_errors_when_asset_does_not_exist() {
    let db = common::setup_test_db().await;

    let order = BuyOrder {
        date: "2025-01-02".to_owned(),
        quantity: 10.0,
        price: 100.0,
        fees: 0.0,
    };

    let err = transactions::buy(&db, "NONEXISTENT".to_owned(), order)
        .await
        .unwrap_err();

    let msg = err.to_string();
    assert!(
        msg.contains("not found") && msg.contains("portfolio asset add"),
        "expected helpful error message, got: {msg}"
    );
}

#[tokio::test]
async fn buy_succeeds_when_asset_exists() {
    let db = common::setup_test_db().await;
    let info = equity_info();

    asset_repo::create(&db, &info, &AssetClassification::default(), None)
        .await
        .unwrap();

    let order = BuyOrder {
        date: "2025-01-02".to_owned(),
        quantity: 10.0,
        price: 100.0,
        fees: 5.0,
    };

    transactions::buy(&db, "XFAKE1".to_owned(), order)
        .await
        .unwrap();
}
