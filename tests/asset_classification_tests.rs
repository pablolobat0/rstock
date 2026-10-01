pub mod common;

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, Set};

use rstock::db::entities::{asset, daily_asset_price, portfolio_asset_history, portfolio_history};
use rstock::db::repos::asset_repo;
use rstock::models::{
    AssetClass, AssetClassification, AssetInfo, AssetType, BondCredit, BondDuration, BuyOrder,
    EquityStyle, Management,
};
use rstock::services::{assets, nav, transactions};

fn equity_info() -> AssetInfo {
    AssetInfo {
        ticker: "XFAKE1".to_owned(),
        name: "Fake ETF".to_owned(),
        asset_type: AssetType::Etf,
        currency: "EUR".to_owned(),
    }
}

#[tokio::test]
async fn create_roundtrips_valid_classification_fields() {
    let db = common::setup_test_db().await;
    let info = equity_info();
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        equity_style: Some(EquityStyle::Blend),
        management: Some(Management::Passive),
        ..Default::default()
    };

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
    assert_eq!(row.bond_credit.as_deref(), None);
    assert_eq!(row.bond_duration.as_deref(), None);
    assert_eq!(row.management.as_deref(), Some("passive"));
}

#[tokio::test]
async fn create_tracked_asset_requires_classification() {
    let db = common::setup_test_db().await;
    let info = equity_info();

    let err = assets::create_tracked_asset(&db, &info, &AssetClassification::default(), None)
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("classification"),
        "expected classification error, got: {err}"
    );
}

#[test]
fn validation_rejects_equity_style_for_non_equity_asset_class() {
    let classification = AssetClassification {
        asset_class: Some(AssetClass::FixedIncome),
        equity_style: Some(EquityStyle::Blend),
        ..Default::default()
    };

    let err = classification
        .validate_for_asset(&AssetType::Stock, None)
        .unwrap_err();

    assert!(
        err.to_string().contains("equity style"),
        "expected equity style error, got: {err}"
    );
}

#[test]
fn validation_rejects_bond_fields_for_non_fixed_income_asset_class() {
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        bond_credit: Some(BondCredit::Government),
        bond_duration: Some(BondDuration::Long),
        ..Default::default()
    };

    let err = classification
        .validate_for_asset(&AssetType::Stock, None)
        .unwrap_err();

    assert!(
        err.to_string().contains("bond credit"),
        "expected bond credit error, got: {err}"
    );
}

#[test]
fn validation_requires_morningstar_code_for_fund_and_etf() {
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        ..Default::default()
    };

    let fund_err = classification
        .validate_for_asset(&AssetType::Fund, None)
        .unwrap_err();
    let etf_err = classification
        .validate_for_asset(&AssetType::Etf, Some(""))
        .unwrap_err();

    assert!(fund_err.to_string().contains("Morningstar code"));
    assert!(etf_err.to_string().contains("Morningstar code"));
    classification
        .validate_for_asset(&AssetType::Stock, None)
        .expect("stock should not require Morningstar code");
}

#[tokio::test]
async fn create_errors_on_duplicate_ticker() {
    let db = common::setup_test_db().await;
    let info = equity_info();
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        ..Default::default()
    };

    assets::create_tracked_asset(&db, &info, &classification, Some("MSTAR123"))
        .await
        .unwrap();

    let err = assets::create_tracked_asset(&db, &info, &classification, Some("MSTAR123"))
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
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        equity_style: Some(EquityStyle::Blend),
        management: Some(Management::Passive),
        ..Default::default()
    };

    asset_repo::create(&db, &info, &classification, Some("MSTAR123"))
        .await
        .unwrap();

    // Update only management and name, leave everything else untouched
    let partial = AssetClassification {
        management: Some(Management::Active),
        ..Default::default()
    };
    assets::update_tracked_asset(&db, "XFAKE1", &partial, Some("New Name"), None)
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
    assert_eq!(row.bond_credit.as_deref(), None);
    assert_eq!(row.bond_duration.as_deref(), None);
    assert_eq!(row.morningstar_code.as_deref(), Some("MSTAR123"));
}

#[tokio::test]
async fn update_keeps_identity_type_and_currency_immutable() {
    let db = common::setup_test_db().await;
    let info = equity_info();
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        ..Default::default()
    };
    asset_repo::create(&db, &info, &classification, Some("MSTAR123"))
        .await
        .unwrap();

    assets::update_tracked_asset(
        &db,
        "XFAKE1",
        &AssetClassification::default(),
        Some("New Name"),
        None,
    )
    .await
    .unwrap();

    let row = asset::Entity::find()
        .filter(asset::Column::Ticker.eq("XFAKE1"))
        .one(&db)
        .await
        .unwrap()
        .expect("asset should exist");

    assert_eq!(row.ticker, "XFAKE1");
    assert_eq!(row.asset_type, "etf");
    assert_eq!(row.currency, "EUR");
    assert_eq!(row.name, "New Name");
}

#[tokio::test]
async fn update_rejects_inconsistent_classification() {
    let db = common::setup_test_db().await;
    let info = AssetInfo {
        ticker: "XFAKE2".to_owned(),
        name: "Fake Stock".to_owned(),
        asset_type: AssetType::Stock,
        currency: "EUR".to_owned(),
    };
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        ..Default::default()
    };
    asset_repo::create(&db, &info, &classification, None)
        .await
        .unwrap();

    let err = assets::update_tracked_asset(
        &db,
        "XFAKE2",
        &AssetClassification {
            bond_credit: Some(BondCredit::Government),
            ..Default::default()
        },
        None,
        None,
    )
    .await
    .unwrap_err();

    assert!(
        err.to_string().contains("bond credit"),
        "expected bond credit validation error, got: {err}"
    );
}

#[tokio::test]
async fn fund_morningstar_code_update_invalidates_price_cache_and_snapshots() {
    let db = common::setup_test_db().await;
    let info = equity_info();
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        ..Default::default()
    };
    let asset_id = asset_repo::create(&db, &info, &classification, Some("OLDMSTAR"))
        .await
        .unwrap();
    common::insert_transaction(&db, asset_id, "2025-01-02", 1.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;
    common::insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 10.0).await;
    portfolio_asset_history::Entity::insert(portfolio_asset_history::ActiveModel {
        date: Set("2025-01-02".to_owned()),
        asset_id: Set(asset_id),
        quantity: Set(1.0),
        closing_price: Set(100.0),
        market_value: Set(100.0),
        exchange_rate: Set(1.0),
        ..Default::default()
    })
    .exec(&db)
    .await
    .unwrap();

    assets::update_tracked_asset(
        &db,
        "XFAKE1",
        &AssetClassification::default(),
        None,
        Some("NEWMSTAR"),
    )
    .await
    .unwrap();

    let price_count = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .all(&db)
        .await
        .unwrap()
        .len();
    let portfolio_count = portfolio_history::Entity::find()
        .all(&db)
        .await
        .unwrap()
        .len();
    let asset_snapshot_count = portfolio_asset_history::Entity::find()
        .filter(portfolio_asset_history::Column::AssetId.eq(asset_id))
        .all(&db)
        .await
        .unwrap()
        .len();

    assert_eq!(price_count, 0);
    assert_eq!(portfolio_count, 0);
    assert_eq!(asset_snapshot_count, 0);
}

#[tokio::test]
async fn non_provider_metadata_update_keeps_price_cache() {
    let db = common::setup_test_db().await;
    let info = equity_info();
    let classification = AssetClassification {
        asset_class: Some(AssetClass::Equity),
        ..Default::default()
    };
    let asset_id = asset_repo::create(&db, &info, &classification, Some("MSTAR123"))
        .await
        .unwrap();
    common::insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;

    assets::update_tracked_asset(
        &db,
        "XFAKE1",
        &AssetClassification::default(),
        Some("New Name"),
        None,
    )
    .await
    .unwrap();

    let price_count = daily_asset_price::Entity::find()
        .filter(daily_asset_price::Column::AssetId.eq(asset_id))
        .all(&db)
        .await
        .unwrap()
        .len();

    assert_eq!(price_count, 1);
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

    asset_repo::create(
        &db,
        &info,
        &AssetClassification {
            asset_class: Some(AssetClass::Equity),
            ..Default::default()
        },
        Some("MSTAR123"),
    )
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

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn monetary_reclassification_rebuilds_nav_and_supports_later_sale() {
    let db = common::setup_test_db().await;
    let anchor = common::insert_asset(&db, "XANCHOR", "Anchor", "stock", "EUR").await;
    let reclassified = assets::create_tracked_asset(
        &db,
        &AssetInfo {
            ticker: "XRECLASS".to_owned(),
            name: "Reclassified asset".to_owned(),
            asset_type: AssetType::Stock,
            currency: "EUR".to_owned(),
        },
        &AssetClassification {
            asset_class: Some(AssetClass::Monetary),
            ..Default::default()
        },
        None,
    )
    .await
    .unwrap();
    common::insert_transaction(&db, anchor, "2025-01-01", 1.0, 100.0, 0.0).await;
    common::insert_transaction(&db, reclassified, "2025-01-02", 10.0, 10.0, 0.0).await;
    for day in 1..=5 {
        let date = format!("2025-01-{day:02}");
        common::insert_daily_price(&db, anchor, &date, 100.0, false).await;
        if day >= 2 {
            let price = if day == 2 { 10.0 } else { 20.0 };
            common::insert_daily_price(&db, reclassified, &date, price, false).await;
        }
    }
    let sources = common::MockMarketDataSources::new();
    let at_checkpoint = common::market_data_at(
        &sources,
        chrono::NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(),
    );
    let original = nav::ensure_portfolio_history(&db, &at_checkpoint)
        .await
        .unwrap()
        .latest_snapshot
        .unwrap();
    assert!((original.total_value - 100.0).abs() < 1e-9);
    let earlier = common::get_portfolio_snapshot(&db, "2025-01-01")
        .await
        .unwrap();

    assets::update_tracked_asset(
        &db,
        "XRECLASS",
        &AssetClassification {
            asset_class: Some(AssetClass::Equity),
            ..Default::default()
        },
        None,
        None,
    )
    .await
    .unwrap();
    let rebuilt = nav::ensure_portfolio_history(&db, &at_checkpoint)
        .await
        .unwrap()
        .latest_snapshot
        .unwrap();
    assert!((rebuilt.total_value - 300.0).abs() < 1e-9);
    assert!((rebuilt.nav - 150.0).abs() < 1e-9);
    assert_eq!(
        common::get_portfolio_snapshot(&db, "2025-01-01")
            .await
            .unwrap(),
        earlier
    );
    let extended = nav::ensure_portfolio_history(
        &db,
        &common::market_data_at(
            &sources,
            chrono::NaiveDate::from_ymd_opt(2025, 1, 5).unwrap(),
        ),
    )
    .await
    .unwrap()
    .latest_snapshot
    .unwrap();
    assert!((extended.total_value - 300.0).abs() < 1e-9);
    transactions::sell(
        &db,
        "XRECLASS".to_owned(),
        rstock::models::SellOrder {
            date: "2025-01-05".to_owned(),
            quantity: 5.0,
            price: 20.0,
            fees: 0.0,
        },
    )
    .await
    .unwrap();
    let sold = nav::ensure_portfolio_history(
        &db,
        &common::market_data_at(
            &sources,
            chrono::NaiveDate::from_ymd_opt(2025, 1, 6).unwrap(),
        ),
    )
    .await
    .unwrap()
    .latest_snapshot
    .unwrap();
    assert!((sold.total_value - 200.0).abs() < 1e-9);
    assert!((sold.nav - 150.0).abs() < 1e-9);
    let rows = common::get_asset_snapshots(&db, "2025-01-05").await;
    assert_eq!(rows.len(), 2);
    let row = rows
        .iter()
        .find(|row| row.asset_id == reclassified)
        .unwrap();
    assert!((row.quantity - 5.0).abs() < 1e-9);
    assert!((row.market_value - 100.0).abs() < 1e-9);
}

#[tokio::test]
async fn performance_reclassification_removes_nav_participation_but_keeps_price_cache() {
    let db = common::setup_test_db().await;
    let anchor = common::insert_asset(&db, "XANCHOR", "Anchor", "stock", "EUR").await;
    let reclassified = common::insert_asset(&db, "XRECLASS", "Reclassified", "stock", "EUR").await;
    common::insert_transaction(&db, anchor, "2025-01-01", 1.0, 100.0, 0.0).await;
    common::insert_transaction(&db, reclassified, "2025-01-02", 10.0, 10.0, 0.0).await;
    for date in ["2025-01-01", "2025-01-02", "2025-01-03", "2025-01-04"] {
        common::insert_daily_price(&db, anchor, date, 100.0, false).await;
        common::insert_daily_price(&db, reclassified, date, 20.0, false).await;
    }
    let sources = common::MockMarketDataSources::new();
    let market_data = common::market_data_at(
        &sources,
        chrono::NaiveDate::from_ymd_opt(2025, 1, 4).unwrap(),
    );
    nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();
    let original = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    assert!((original.total_value - 300.0).abs() < 1e-9);
    assets::update_tracked_asset(
        &db,
        "XRECLASS",
        &AssetClassification {
            asset_class: Some(AssetClass::Monetary),
            ..Default::default()
        },
        None,
        None,
    )
    .await
    .unwrap();
    // All parts of a complete date are invalidated, including the unchanged anchor.
    assert!(common::get_asset_snapshots(&db, "2025-01-03")
        .await
        .is_empty());
    let rebuilt = nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap()
        .latest_snapshot
        .unwrap();
    assert!((rebuilt.total_value - 100.0).abs() < 1e-9);
    assert!((rebuilt.nav - 100.0).abs() < 1e-9);
    let extended = nav::ensure_portfolio_history(
        &db,
        &common::market_data_at(
            &sources,
            chrono::NaiveDate::from_ymd_opt(2025, 1, 5).unwrap(),
        ),
    )
    .await
    .unwrap()
    .latest_snapshot
    .unwrap();
    assert!((extended.total_value - 100.0).abs() < 1e-9);
    let rows = common::get_asset_snapshots(&db, "2025-01-04").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].asset_id, anchor);
    assert_eq!(
        daily_asset_price::Entity::find()
            .filter(daily_asset_price::Column::AssetId.eq(reclassified))
            .all(&db)
            .await
            .unwrap()
            .len(),
        4
    );
    // A metadata-only edit must keep the authoritative checkpoint and its detail.
    assets::update_tracked_asset(
        &db,
        "XRECLASS",
        &AssetClassification::default(),
        Some("Renamed"),
        None,
    )
    .await
    .unwrap();
    assert!(
        (common::get_portfolio_snapshot(&db, "2025-01-04")
            .await
            .unwrap()
            .total_value
            - extended.total_value)
            .abs()
            < 1e-9
    );
    assert_eq!(common::get_asset_snapshots(&db, "2025-01-04").await, rows);
}

#[tokio::test]
async fn asset_edits_roll_back_when_complete_snapshot_invalidation_fails() {
    for (initial_class, updated_class, provider_code) in [
        (AssetClass::Monetary, AssetClass::Equity, None),
        (AssetClass::Equity, AssetClass::Monetary, None),
        (AssetClass::Equity, AssetClass::Equity, Some("NEWMSTAR")),
        (AssetClass::Monetary, AssetClass::Equity, Some("NEWMSTAR")),
    ] {
        let db = common::setup_test_db().await;
        let id = assets::create_tracked_asset(
            &db,
            &equity_info(),
            &AssetClassification {
                asset_class: Some(initial_class),
                ..Default::default()
            },
            Some("OLDMSTAR"),
        )
        .await
        .unwrap();
        common::insert_transaction(&db, id, "2025-01-02", 1.0, 100.0, 0.0).await;
        common::insert_daily_price(&db, id, "2025-01-02", 100.0, false).await;
        common::insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 1.0).await;
        common::insert_portfolio_asset_snapshot(&db, "2025-01-02", id, 1.0, 100.0, 100.0, 1.0)
            .await;
        let original_asset = asset::Entity::find_by_id(id)
            .one(&db)
            .await
            .unwrap()
            .unwrap();
        let original_history = common::get_all_snapshots(&db).await;
        let original_rows = common::get_asset_snapshots(&db, "2025-01-02").await;
        let original_prices = daily_asset_price::Entity::find().all(&db).await.unwrap();
        // Fail late, after the metadata update, optional price deletion and portfolio deletion.
        db.execute_unprepared("CREATE TRIGGER fail_asset_snapshot_invalidation BEFORE DELETE ON portfolio_asset_history \
            BEGIN SELECT RAISE(ABORT, 'injected asset invalidation failure'); END")
            .await.unwrap();
        let error = assets::update_tracked_asset(
            &db,
            "XFAKE1",
            &AssetClassification {
                asset_class: Some(updated_class),
                ..Default::default()
            },
            Some("Should roll back"),
            provider_code,
        )
        .await
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("injected asset invalidation failure"));
        assert_eq!(
            asset::Entity::find_by_id(id)
                .one(&db)
                .await
                .unwrap()
                .unwrap(),
            original_asset
        );
        assert_eq!(common::get_all_snapshots(&db).await, original_history);
        assert_eq!(
            common::get_asset_snapshots(&db, "2025-01-02").await,
            original_rows
        );
        assert_eq!(
            daily_asset_price::Entity::find().all(&db).await.unwrap(),
            original_prices
        );
    }
}
