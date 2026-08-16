mod common;

use chrono::NaiveDate;
use rstock::models::{
    BuyOrder, MarketDataLimitation, MarketDataLimitationClassification, MarketDataSubject,
};
use rstock::services::{nav, transactions};
use sea_orm::ConnectionTrait;

fn nav_market_data(
    sources: &common::MockMarketDataSources,
) -> rstock::services::market_data::MarketData {
    common::market_data_at(sources, NaiveDate::from_ymd_opt(2025, 2, 1).unwrap())
}

/// No transactions -> readiness returns Ok, no portfolio_history rows.
#[tokio::test]
async fn test_empty_portfolio() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snapshots = common::get_all_snapshots(&db).await;
    assert!(snapshots.is_empty());
}

/// One buy -> NAV starts at 100.0, outstanding_shares = deposit / 100.
#[tokio::test]
async fn test_single_buy_initial_nav() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    // Buy 10 shares at $50 = $500 deposit, 0 fees
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    // EOD price is $50
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .expect("snapshot should exist");

    // deposit = 10 * 50 = 500
    // initial NAV = 100, shares_issued = 500 / 100 = 5
    assert_eq!(snap.outstanding_shares, 5.0);
    // EOD: asset_value = 10 * 50 = 500, nav = 500 / 5 = 100
    assert!((snap.nav - 100.0).abs() < 0.01);
    assert!((snap.asset_value - 500.0).abs() < 0.01);
    assert!((snap.total_value - 500.0).abs() < 0.01);
}

/// Buy day 1, price doubles day 2 -> NAV doubles, shares unchanged.
#[tokio::test]
async fn test_nav_reflects_price_change() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap_d1 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    let snap_d2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();

    // Shares unchanged
    assert_eq!(snap_d1.outstanding_shares, snap_d2.outstanding_shares);
    // NAV doubled: 100 -> 200
    assert!((snap_d2.nav - 200.0).abs() < 0.01);
    // Asset value doubled: 500 -> 1000
    assert!((snap_d2.asset_value - 1000.0).abs() < 0.01);
}

/// Buy day 1, buy day 5 -> NAV on day 5 uses previous day's EOD NAV
/// for share issuance; outstanding_shares increases.
#[tokio::test]
async fn test_second_buy_no_nav_jump() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    // Day 1: buy 10 @ $50
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    // Day 5: buy 10 @ $60
    common::insert_transaction(&db, asset_id, "2025-01-06", 10.0, 60.0, 0.0).await;

    // Prices: stable at 50, then 60 on day 5
    for (date, price) in [
        ("2025-01-02", 50.0),
        ("2025-01-03", 50.0),
        ("2025-01-04", 50.0),
        ("2025-01-05", 50.0),
        ("2025-01-06", 60.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap_d4 = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .unwrap();
    let snap_d5 = common::get_portfolio_snapshot(&db, "2025-01-06")
        .await
        .unwrap();

    // Day 4 (before second buy): 10 shares @ 50 = 500, 5 outstanding, NAV = 100
    assert_eq!(snap_d4.outstanding_shares, 5.0);
    assert!((snap_d4.nav - 100.0).abs() < 0.01);

    // Day 5: price jumped to 60
    // The NAV engine uses previous day's EOD NAV for share issuance.
    // Pre-buy NAV (from Jan 5 EOD) = 100 (price was still 50)
    // Second buy deposit = 10 * 60 = 600, shares_issued = 600 / 100 = 6
    // outstanding = 5 + 6 = 11, holdings = 20
    // EOD: 20 * 60 = 1200, NAV = 1200 / 11 ~ 109.09
    assert!(snap_d5.outstanding_shares > snap_d4.outstanding_shares);
    assert!((snap_d5.outstanding_shares - 11.0).abs() < 0.01);
    assert!((snap_d5.asset_value - 1200.0).abs() < 0.01);
    assert!((snap_d5.nav - (1200.0 / 11.0)).abs() < 0.01);
}

/// Two buys on same day -> shares accumulate correctly.
#[tokio::test]
async fn test_same_day_multiple_buys() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    // Two buys on same day
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 5.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();

    // First buy: deposit=500, NAV=100, shares=5, holdings=10
    // Second buy: deposit=250, NAV=100, shares_issued=2.5, outstanding=7.5, holdings=15
    // EOD: 15 * 50 = 750, NAV = 750 / 7.5 = 100
    assert!((snap.outstanding_shares - 7.5).abs() < 0.01);
    assert!((snap.asset_value - 750.0).abs() < 0.01);
    assert!((snap.nav - 100.0).abs() < 0.01);
}

/// Buy on Friday -> Saturday/Sunday get Friday's price via forward-fill.
#[tokio::test]
async fn test_weekend_forward_fill() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    // 2025-01-03 is a Friday; 2025-01-06 is the next Monday.
    // API returns Friday + Monday; Sat/Sun get forward-filled between trading days.
    mock.historical_prices.insert(
        "XFAKE1".to_string(),
        vec![
            ("2025-01-03".to_string(), 50.0),
            ("2025-01-06".to_string(), 50.0),
        ],
    );
    common::insert_transaction(&db, asset_id, "2025-01-03", 10.0, 50.0, 0.0).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap_fri = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    let snap_sat = common::get_portfolio_snapshot(&db, "2025-01-04")
        .await
        .unwrap();
    let snap_sun = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .unwrap();

    // All should have same NAV = 100 (forward-fill from Friday's price)
    assert!((snap_fri.nav - 100.0).abs() < 0.01);
    assert!((snap_sat.nav - 100.0).abs() < 0.01);
    assert!((snap_sun.nav - 100.0).abs() < 0.01);
    // Same outstanding shares
    assert_eq!(snap_fri.outstanding_shares, snap_sat.outstanding_shares);
    assert_eq!(snap_fri.outstanding_shares, snap_sun.outstanding_shares);
}

/// Extending ready history leaves earlier snapshots unchanged.
#[tokio::test]
async fn test_incremental_readiness_preserves_existing_history() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    for (date, price) in [
        ("2025-01-02", 50.0),
        ("2025-01-03", 55.0),
        ("2025-01-04", 60.0),
        ("2025-01-05", 65.0),
    ] {
        common::insert_daily_price(&db, asset_id, date, price, false).await;
    }

    // Build history through Jan 3.
    nav::ensure_portfolio_history(
        &db,
        &common::market_data_at(&mock, NaiveDate::from_ymd_opt(2025, 1, 4).unwrap()),
    )
    .await
    .unwrap();

    let snap_d2_before = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();

    // A later clock extends history from day 4 without touching prior snapshots.
    nav::ensure_portfolio_history(
        &db,
        &common::market_data_at(&mock, NaiveDate::from_ymd_opt(2025, 1, 6).unwrap()),
    )
    .await
    .unwrap();

    let snap_d2_after = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    let snap_d3_after = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();

    // Day 2 and 3 should be unchanged
    assert_eq!(snap_d2_before.nav, snap_d2_after.nav);
    assert_eq!(
        snap_d2_before.outstanding_shares,
        snap_d2_after.outstanding_shares
    );

    // Day 4 and 5 should exist with correct values
    let snap_d4 = common::get_portfolio_snapshot(&db, "2025-01-04")
        .await
        .unwrap();
    // 10 shares @ 60 = 600, 5 outstanding, NAV = 120
    assert!((snap_d4.asset_value - 600.0).abs() < 0.01);
    assert!((snap_d4.nav - 120.0).abs() < 0.01);

    // Day 3 should still be there
    assert!((snap_d3_after.nav - 110.0).abs() < 0.01);
}

/// A failed asset write rolls back the active batch, while a later readiness
/// call resumes after the last complete NAV snapshot.
#[tokio::test]
async fn interrupted_nav_batch_preserves_complete_history_and_resumes() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();
    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;

    let mut date = NaiveDate::from_ymd_opt(2025, 1, 2).unwrap();
    let end = NaiveDate::from_ymd_opt(2025, 5, 31).unwrap();
    while date <= end {
        common::insert_daily_price(&db, asset_id, &date.to_string(), 50.0, false).await;
        date += chrono::Duration::days(1);
    }

    db.execute_unprepared(
        "CREATE TRIGGER fail_nav_asset_batch BEFORE INSERT ON portfolio_asset_history \
         WHEN NEW.date = '2025-04-11' \
         BEGIN SELECT RAISE(ABORT, 'injected NAV batch failure'); END",
    )
    .await
    .unwrap();

    let market_data = common::market_data_at(&mock, NaiveDate::from_ymd_opt(2025, 6, 1).unwrap());
    assert!(nav::ensure_portfolio_history(&db, &market_data)
        .await
        .is_err());

    let snapshots = common::get_all_snapshots(&db).await;
    assert_eq!(
        snapshots.last().map(|snapshot| snapshot.date.as_str()),
        Some("2025-04-10")
    );
    assert!(snapshots.iter().all(|snapshot| {
        snapshot.date == "2025-01-01"
            || ((snapshot.nav - 100.0).abs() < 0.01
                && (snapshot.asset_value - 500.0).abs() < 0.01
                && (snapshot.total_value - 500.0).abs() < 0.01)
    }));
    for snapshot in &snapshots {
        if snapshot.date != "2025-01-01" {
            assert_eq!(
                common::get_asset_snapshots(&db, &snapshot.date).await.len(),
                1
            );
        }
    }
    assert!(common::get_portfolio_snapshot(&db, "2025-04-11")
        .await
        .is_none());

    db.execute_unprepared("DROP TRIGGER fail_nav_asset_batch")
        .await
        .unwrap();
    nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    let snapshots = common::get_all_snapshots(&db).await;
    assert_eq!(snapshots.len(), 151);
    assert_eq!(
        snapshots.last().map(|snapshot| snapshot.date.as_str()),
        Some("2025-05-31")
    );
    assert!(snapshots.iter().all(|snapshot| {
        snapshot.date == "2025-01-01"
            || ((snapshot.nav - 100.0).abs() < 0.01
                && (snapshot.asset_value - 500.0).abs() < 0.01
                && (snapshot.total_value - 500.0).abs() < 0.01)
    }));
    for snapshot in snapshots {
        if snapshot.date != "2025-01-01" {
            assert_eq!(
                common::get_asset_snapshots(&db, &snapshot.date).await.len(),
                1
            );
        }
    }
}

#[tokio::test]
async fn incomplete_latest_snapshot_is_discarded_before_resuming() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();
    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    for date in ["2025-01-02", "2025-01-03"] {
        common::insert_daily_price(&db, asset_id, date, 50.0, false).await;
    }
    common::insert_portfolio_snapshot(&db, "2025-01-01", 100.0, 0.0).await;
    common::insert_portfolio_asset_snapshot(&db, "2025-01-02", asset_id, 10.0, 50.0, 500.0, 1.0)
        .await;
    common::insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 5.0).await;
    common::insert_portfolio_snapshot(&db, "2025-01-03", 100.0, 5.0).await;

    let market_data = common::market_data_at(&mock, NaiveDate::from_ymd_opt(2025, 2, 1).unwrap());
    nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    assert!(common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .is_some());
    assert_eq!(
        common::get_asset_snapshots(&db, "2025-01-03").await.len(),
        1
    );
}

/// A backdated buy invalidates and correctly recomputes history from its date.
#[tokio::test]
async fn test_back_dated_buy() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;

    // Prices for 5 days (all at 50)
    for day in 2..=6 {
        let date = format!("2025-01-{:02}", day);
        common::insert_daily_price(&db, asset_id, &date, 50.0, false).await;
    }

    // Build initial history
    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap_d5_before = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .unwrap();
    assert_eq!(snap_d5_before.outstanding_shares, 5.0); // 500/100

    // The accepted transaction interface invalidates snapshots from the backdated buy onward.
    transactions::buy(
        &db,
        "XFAKE1".to_owned(),
        BuyOrder {
            date: "2025-01-03".to_owned(),
            quantity: 10.0,
            price: 50.0,
            fees: 0.0,
        },
    )
    .await
    .unwrap();

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap_d5_after = common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .unwrap();

    // Now outstanding_shares should be higher: first buy 5 + second buy 5 = 10
    assert!((snap_d5_after.outstanding_shares - 10.0).abs() < 0.01);
    // Holdings: 20 shares @ 50 = 1000, NAV = 1000/10 = 100
    assert!((snap_d5_after.asset_value - 1000.0).abs() < 0.01);
    assert!((snap_d5_after.nav - 100.0).abs() < 0.01);
}

/// Two assets with different prices -> asset_value = sum of both.
#[tokio::test]
async fn test_multiple_assets() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_a = common::insert_asset(&db, "XFAKE1", "Asset A", "stock", "EUR").await;
    let asset_b = common::insert_asset(&db, "XFAKE2", "Asset B", "stock", "EUR").await;

    // Buy 10 of A @ $50 and 5 of B @ $100
    common::insert_transaction(&db, asset_a, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_transaction(&db, asset_b, "2025-01-02", 5.0, 100.0, 0.0).await;

    common::insert_daily_price(&db, asset_a, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_b, "2025-01-02", 100.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();

    // Total deposit = 500 + 500 = 1000
    // First buy: NAV=100, shares=5 (500/100)
    // Second buy: NAV still 100, shares = 500/100 = 5
    // Total outstanding = 10
    // EOD: asset_value = 10*50 + 5*100 = 1000
    // NAV = 1000 / 10 = 100
    assert!((snap.outstanding_shares - 10.0).abs() < 0.01);
    assert!((snap.nav - 100.0).abs() < 0.01);
    assert!((snap.asset_value - 1000.0).abs() < 0.01);
}

/// Asset has no market data -> NAV readiness is unavailable (no snapshot, no
/// partial snapshots written) with an actionable Asset limitation.
#[tokio::test]
async fn test_missing_price_for_asset_fails_without_partial_snapshots() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;

    let readiness = nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .expect("unavailable historical market data is a tolerated NAV outcome");

    assert!(readiness.latest_snapshot.is_none());
    assert_eq!(
        readiness.market_data_limitations,
        vec![MarketDataLimitation {
            subject: MarketDataSubject::Asset {
                ticker: "XFAKE1".to_owned(),
                name: "Test Stock".to_owned(),
                asset_type: rstock::models::AssetType::Stock,
            },
            latest_available_date: None,
            requested_end_date: NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
            classification: MarketDataLimitationClassification::ActionableMissingData,
        }]
    );
    assert!(common::get_all_snapshots(&db).await.is_empty());
}

#[tokio::test]
async fn test_missing_first_day_asset_valuation_is_unavailable_without_seed_snapshot() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    mock.historical_prices
        .insert("XFAKE1".to_owned(), vec![("2025-01-03".to_owned(), 50.0)]);

    let readiness = nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .expect("missing initial valuation is a readable NAV outcome");

    assert!(readiness.latest_snapshot.is_none());
    assert!(readiness.market_data_limitations.iter().any(|limitation| {
        matches!(
            limitation.subject,
            MarketDataSubject::Asset { ref ticker, .. } if ticker == "XFAKE1"
        ) && limitation.classification == MarketDataLimitationClassification::ActionableMissingData
    }));
    assert!(common::get_all_snapshots(&db).await.is_empty());
}

#[tokio::test]
async fn test_incremental_missing_first_day_asset_valuation_preserves_existing_history() {
    let db = common::setup_test_db().await;
    let held_asset = common::insert_asset(&db, "XFAKEOLD", "Existing Stock", "stock", "EUR").await;
    let new_asset = common::insert_asset(&db, "XFAKENEW", "New Stock", "stock", "EUR").await;
    common::insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 1.0).await;
    common::insert_portfolio_asset_snapshot(&db, "2025-01-02", held_asset, 1.0, 100.0, 100.0, 1.0)
        .await;
    common::insert_daily_price(&db, held_asset, "2025-01-02", 100.0, false).await;
    common::insert_transaction(&db, new_asset, "2025-01-03", 1.0, 50.0, 0.0).await;

    let mut mock = common::MockMarketDataSources::new();
    mock.historical_prices
        .insert("XFAKENEW".to_owned(), vec![("2025-01-04".to_owned(), 50.0)]);

    let readiness = nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .expect("missing incremental valuation is a readable NAV outcome");

    assert_eq!(readiness.latest_snapshot.unwrap().date, "2025-01-02");
    assert!(readiness.market_data_limitations.iter().any(|limitation| {
        matches!(
            limitation.subject,
            MarketDataSubject::Asset { ref ticker, .. } if ticker == "XFAKENEW"
        ) && limitation.classification == MarketDataLimitationClassification::ActionableMissingData
    }));
    assert!(common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .is_none());
}

/// Non-EUR asset with no FX data -> NAV readiness is unavailable without a
/// snapshot or partial data, with an actionable FX limitation alongside the
/// stale price limitation for the valued asset.
#[tokio::test]
async fn test_missing_fx_rate_fails_without_partial_snapshots() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKEUSD", "US Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    mock.historical_prices.insert(
        "XFAKEUSD".to_owned(),
        vec![("2025-01-02".to_owned(), 100.0)],
    );

    let readiness = nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .expect("unavailable FX data is a readable NAV outcome");

    assert!(readiness.latest_snapshot.is_none());
    assert_eq!(
        readiness.market_data_limitations,
        vec![
            MarketDataLimitation {
                subject: MarketDataSubject::Asset {
                    ticker: "XFAKEUSD".to_owned(),
                    name: "US Stock".to_owned(),
                    asset_type: rstock::models::AssetType::Stock.into(),
                },
                latest_available_date: Some(NaiveDate::from_ymd_opt(2025, 1, 2).unwrap()),
                requested_end_date: NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
                classification: MarketDataLimitationClassification::ActionableStaleData,
            },
            MarketDataLimitation {
                subject: MarketDataSubject::FxRate {
                    currency: "USD".to_owned(),
                },
                latest_available_date: None,
                requested_end_date: NaiveDate::from_ymd_opt(2025, 1, 31).unwrap(),
                classification: MarketDataLimitationClassification::ActionableMissingData,
            },
        ]
    );
    assert!(common::get_all_snapshots(&db).await.is_empty());
}

/// Effective valuation date is the minimum latest date across required prices and FX rates.
#[tokio::test]
async fn test_effective_valuation_date_uses_minimum_required_market_data_date() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let eur_id = common::insert_asset(&db, "XFAKEEUR", "EUR Stock", "stock", "EUR").await;
    let usd_id = common::insert_asset(&db, "XFAKEUSD", "US Stock", "stock", "USD").await;

    common::insert_transaction(&db, eur_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_transaction(&db, usd_id, "2025-01-02", 5.0, 100.0, 0.0).await;

    mock.historical_prices.insert(
        "XFAKEEUR".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 50.0),
            ("2025-01-03".to_owned(), 50.0),
            ("2025-01-04".to_owned(), 50.0),
            ("2025-01-05".to_owned(), 50.0),
        ],
    );
    mock.historical_prices.insert(
        "XFAKEUSD".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 100.0),
            ("2025-01-03".to_owned(), 100.0),
            ("2025-01-04".to_owned(), 100.0),
        ],
    );
    mock.exchange_rates.insert(
        "USDEUR".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 0.90),
            ("2025-01-03".to_owned(), 0.90),
            ("2025-01-04".to_owned(), 0.90),
            ("2025-01-05".to_owned(), 0.90),
            ("2025-01-06".to_owned(), 0.90),
        ],
    );

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    assert!(common::get_portfolio_snapshot(&db, "2025-01-04")
        .await
        .is_some());
    assert!(common::get_portfolio_snapshot(&db, "2025-01-05")
        .await
        .is_none());
}

#[tokio::test]
async fn test_nav_market_data_uses_stock_ticker_for_price_lookup() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKESTOCK", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    mock.historical_prices.insert(
        "XFAKESTOCK".to_owned(),
        vec![("2025-01-02".to_owned(), 50.0)],
    );

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    assert!(common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .is_some());
}

#[tokio::test]
async fn test_nav_market_data_uses_fund_morningstar_code_for_price_lookup() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset_id =
        common::insert_fund_asset(&db, "XFAKEFUND", "Test Fund", "EUR", "MSTARFUND").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    mock.historical_prices.insert(
        "MSTARFUND".to_owned(),
        vec![("2025-01-02".to_owned(), 50.0)],
    );

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    assert!(common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .is_some());
}

#[tokio::test]
async fn test_nav_market_data_uses_etf_morningstar_code_for_price_lookup() {
    let db = common::setup_test_db().await;
    let mut mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_etf_asset(&db, "XFAKEETF", "Test ETF", "EUR", "MSTARETF").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    mock.historical_prices
        .insert("MSTARETF".to_owned(), vec![("2025-01-02".to_owned(), 50.0)]);

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    assert!(common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .is_some());
}

#[tokio::test]
async fn test_nav_market_data_fails_when_fund_morningstar_code_is_missing() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKEFUND", "Test Fund", "fund", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;

    let result = nav::ensure_portfolio_history(&db, &nav_market_data(&mock)).await;

    let error = result.expect_err("missing Morningstar code should fail NAV preparation");
    assert!(error
        .to_string()
        .contains("missing Morningstar code for required fund XFAKEFUND (Test Fund)"));
    assert!(common::get_all_snapshots(&db).await.is_empty());
}

// ========== NEW TESTS ==========

/// After rebuild, portfolio_asset_history rows exist with correct values.
#[tokio::test]
async fn test_per_asset_history_created() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 55.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let asset_snaps_d1 = common::get_asset_snapshots(&db, "2025-01-02").await;
    assert_eq!(asset_snaps_d1.len(), 1);
    assert_eq!(asset_snaps_d1[0].asset_id, asset_id);
    assert!((asset_snaps_d1[0].quantity - 10.0).abs() < 0.01);
    assert!((asset_snaps_d1[0].closing_price - 50.0).abs() < 0.01);
    assert!((asset_snaps_d1[0].market_value - 500.0).abs() < 0.01);

    let asset_snaps_d2 = common::get_asset_snapshots(&db, "2025-01-03").await;
    assert_eq!(asset_snaps_d2.len(), 1);
    assert!((asset_snaps_d2[0].closing_price - 55.0).abs() < 0.01);
    assert!((asset_snaps_d2[0].market_value - 550.0).abs() < 0.01);
}

/// Two assets -> each has its own row per day in portfolio_asset_history.
#[tokio::test]
async fn test_per_asset_history_multiple_assets() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_a = common::insert_asset(&db, "XFAKE1", "Asset A", "stock", "EUR").await;
    let asset_b = common::insert_asset(&db, "XFAKE2", "Asset B", "stock", "EUR").await;

    common::insert_transaction(&db, asset_a, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_transaction(&db, asset_b, "2025-01-02", 5.0, 100.0, 0.0).await;

    common::insert_daily_price(&db, asset_a, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_b, "2025-01-02", 100.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let asset_snaps = common::get_asset_snapshots(&db, "2025-01-02").await;
    assert_eq!(asset_snaps.len(), 2);

    // Sorted by asset_id, so asset_a (id=1) first, asset_b (id=2) second
    let snap_a = &asset_snaps[0];
    let snap_b = &asset_snaps[1];

    assert_eq!(snap_a.asset_id, asset_a);
    assert!((snap_a.quantity - 10.0).abs() < 0.01);
    assert!((snap_a.closing_price - 50.0).abs() < 0.01);
    assert!((snap_a.market_value - 500.0).abs() < 0.01);

    assert_eq!(snap_b.asset_id, asset_b);
    assert!((snap_b.quantity - 5.0).abs() < 0.01);
    assert!((snap_b.closing_price - 100.0).abs() < 0.01);
    assert!((snap_b.market_value - 500.0).abs() < 0.01);
}

/// Insert transaction directly -> portfolio_history is empty until readiness is requested.
#[tokio::test]
async fn test_lazy_history_readiness() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;

    // Readiness has not been requested yet, so portfolio_history should be empty.
    let snapshots = common::get_all_snapshots(&db).await;
    assert!(snapshots.is_empty());

    // The public readiness interface populates history lazily.
    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    // portfolio_history should now have rows
    let snapshots = common::get_all_snapshots(&db).await;
    assert!(!snapshots.is_empty());

    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    assert!((snap.nav - 100.0).abs() < 0.01);
}

/// Buy a USD stock with USDEUR exchange rate. Verify NAV is computed in EUR.
#[tokio::test]
async fn test_single_usd_asset_nav() {
    let db = common::setup_test_db().await;

    let asset_id = common::insert_asset(&db, "XFAKEUSD", "US Stock", "stock", "USD").await;
    // Buy 10 shares @ $100 USD
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;

    let mut mock = common::MockMarketDataSources::new();
    mock.historical_prices.insert(
        "XFAKEUSD".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 100.0),
            ("2025-01-03".to_owned(), 110.0),
        ],
    );
    mock.exchange_rates.insert(
        "USDEUR".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 0.90),
            ("2025-01-03".to_owned(), 0.92),
        ],
    );

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    // Day 1: deposit = 10 * 100 * 0.90 = 900 EUR, NAV = 100, shares = 9
    let snap1 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    assert!((snap1.nav - 100.0).abs() < 0.01);
    assert!((snap1.outstanding_shares - 9.0).abs() < 0.01);
    // total_value = 10 * 100 * 0.90 = 900
    assert!((snap1.total_value - 900.0).abs() < 0.01);

    // Day 2: price goes to $110, rate to 0.92
    // total_value = 10 * 110 * 0.92 = 1012
    // NAV = 1012 / 9 = 112.44...
    let snap2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    assert!((snap2.total_value - 1012.0).abs() < 0.01);
    assert!((snap2.nav - (1012.0 / 9.0)).abs() < 0.01);
}

/// A genuine readiness/preparation error is not swallowed into a readable NAV:
/// a fund without its Morningstar code must propagate as an Err rather than be
/// masked as an unavailable-NAV limitation.
#[tokio::test]
async fn ensure_portfolio_history_propagates_genuine_errors() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKEFUND", "Test Fund", "fund", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;

    let result = nav::ensure_portfolio_history(&db, &nav_market_data(&mock)).await;

    let error = result.expect_err("missing Morningstar code must not be masked by availability");
    assert!(error
        .to_string()
        .contains("missing Morningstar code for required fund XFAKEFUND (Test Fund)"));
    assert!(common::get_all_snapshots(&db).await.is_empty());
}

/// Mixed EUR + USD portfolio. Verify deposits and valuations convert correctly.
#[tokio::test]
async fn test_mixed_currency_portfolio() {
    let db = common::setup_test_db().await;

    let eur_id = common::insert_fund_asset(&db, "XFAKEEUR", "EUR Fund", "EUR", "XFAKEEUR").await;
    let usd_id = common::insert_asset(&db, "XFAKEUSD", "USD Stock", "stock", "USD").await;

    // Buy EUR fund: 10 shares @ 50 EUR
    common::insert_transaction(&db, eur_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    // Buy USD stock: 5 shares @ $200 USD
    common::insert_transaction(&db, usd_id, "2025-01-02", 5.0, 200.0, 0.0).await;

    let mut mock = common::MockMarketDataSources::new();
    mock.historical_prices
        .insert("XFAKEEUR".to_owned(), vec![("2025-01-02".to_owned(), 50.0)]);
    mock.historical_prices.insert(
        "XFAKEUSD".to_owned(),
        vec![("2025-01-02".to_owned(), 200.0)],
    );
    mock.exchange_rates
        .insert("USDEUR".to_owned(), vec![("2025-01-02".to_owned(), 0.90)]);

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    // EUR deposit: 10 * 50 = 500 EUR
    // USD deposit: 5 * 200 * 0.90 = 900 EUR
    // Total deposit: 1400 EUR, NAV = 100, shares = 14
    // EUR value: 10 * 50 = 500
    // USD value: 5 * 200 * 0.90 = 900
    // Total value: 1400, NAV = 1400/14 = 100
    let snap = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    assert!((snap.total_value - 1400.0).abs() < 0.01);
    assert!((snap.nav - 100.0).abs() < 0.01);
    assert!((snap.outstanding_shares - 14.0).abs() < 0.01);

    // Check per-asset snapshots
    let asset_snaps = common::get_asset_snapshots(&db, "2025-01-02").await;
    assert_eq!(asset_snaps.len(), 2);

    let eur_snap = asset_snaps.iter().find(|s| s.asset_id == eur_id).unwrap();
    assert!((eur_snap.market_value - 500.0).abs() < 0.01);
    assert!((eur_snap.exchange_rate - 1.0).abs() < 0.01);

    let usd_snap = asset_snaps.iter().find(|s| s.asset_id == usd_id).unwrap();
    assert!((usd_snap.market_value - 900.0).abs() < 0.01);
    assert!((usd_snap.exchange_rate - 0.90).abs() < 0.01);
}

/// EUR-only portfolio works identically to before (regression test).
#[tokio::test]
async fn test_eur_only_unchanged() {
    let db = common::setup_test_db().await;

    let asset_id = common::insert_asset(&db, "XFAKE1", "EUR Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;

    let mut mock = common::MockMarketDataSources::new();
    mock.historical_prices.insert(
        "XFAKE1".to_owned(),
        vec![
            ("2025-01-02".to_owned(), 50.0),
            ("2025-01-03".to_owned(), 55.0),
        ],
    );

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap1 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    assert!((snap1.nav - 100.0).abs() < 0.01);
    assert!((snap1.total_value - 500.0).abs() < 0.01);

    let snap2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    assert!((snap2.total_value - 550.0).abs() < 0.01);
    assert!((snap2.nav - 110.0).abs() < 0.01);

    // exchange_rate should be 1.0 for EUR assets
    let asset_snaps = common::get_asset_snapshots(&db, "2025-01-02").await;
    assert_eq!(asset_snaps.len(), 1);
    assert!((asset_snaps[0].exchange_rate - 1.0).abs() < 1e-9);
}

// ========== SELL TESTS ==========

/// Buy 10 shares, sell 5 -> asset snapshot shows quantity=5.
#[tokio::test]
async fn test_sell_reduces_holdings() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-01-03", 5.0, 50.0, 0.0).await;

    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 50.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let asset_snaps = common::get_asset_snapshots(&db, "2025-01-03").await;
    assert_eq!(asset_snaps.len(), 1);
    assert!((asset_snaps[0].quantity - 5.0).abs() < 0.01);
    assert!((asset_snaps[0].market_value - 250.0).abs() < 0.01);
}

/// Buy 10 @ $50, sell 5 @ $50 (flat price). NAV should stay at 100.
#[tokio::test]
async fn test_sell_nav_unchanged_at_fair_value() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-01-03", 5.0, 50.0, 0.0).await;

    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 50.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap_d1 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    let snap_d2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();

    // Day 1: 10 * 50 = 500, os = 5, NAV = 100
    assert!((snap_d1.nav - 100.0).abs() < 0.01);

    // Day 2: sell 5 @ 50 = withdrawal 250, shares_redeemed = 250/100 = 2.5
    // os = 5 - 2.5 = 2.5, holdings = 5, value = 5*50 = 250
    // NAV = 250 / 2.5 = 100 (unchanged!)
    assert!((snap_d2.nav - 100.0).abs() < 0.01);
    assert!((snap_d2.outstanding_shares - 2.5).abs() < 0.01);
    assert!((snap_d2.asset_value - 250.0).abs() < 0.01);
}

/// Buy 10 @ $50, price rises to $100, sell 5. NAV should stay at ~200.
#[tokio::test]
async fn test_sell_preserves_nav_after_gain() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-01-04", 5.0, 100.0, 0.0).await;

    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 100.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-04", 100.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap_d2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    let snap_d3 = common::get_portfolio_snapshot(&db, "2025-01-04")
        .await
        .unwrap();

    // Day 2: NAV = 200 (price doubled)
    assert!((snap_d2.nav - 200.0).abs() < 0.01);

    // Day 3: sell 5 @ 100 = withdrawal 500, redeemed = 500/200 = 2.5
    // os = 5 - 2.5 = 2.5, holdings = 5, value = 5*100 = 500
    // NAV = 500/2.5 = 200 (preserved!)
    assert!((snap_d3.nav - 200.0).abs() < 0.01);
    assert!((snap_d3.outstanding_shares - 2.5).abs() < 0.01);
}

/// Sell with fees -> fewer shares redeemed (fees reduce proceeds).
#[tokio::test]
async fn test_sell_with_fees() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    // Sell 5 @ $50 with $10 fees -> proceeds = 250 - 10 = 240
    common::insert_sell_transaction(&db, asset_id, "2025-01-03", 5.0, 50.0, 10.0).await;

    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 50.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();

    // Day 1: os = 5, NAV = 100
    // Day 2: withdrawal = 250 - 10 = 240, redeemed = 240/100 = 2.4
    // os = 5 - 2.4 = 2.6, holdings = 5, value = 5*50 = 250
    // NAV = 250/2.6 = 96.15... (slightly lower due to fees eating into portfolio)
    assert!((snap.outstanding_shares - 2.6).abs() < 0.01);
    assert!((snap.asset_value - 250.0).abs() < 0.01);
    assert!((snap.nav - (250.0 / 2.6)).abs() < 0.01);
}

/// Full liquidation: sell entire position.
#[tokio::test]
async fn test_full_liquidation() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-01-03", 10.0, 50.0, 0.0).await;

    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 50.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();

    // Full sell: withdrawal = 500, redeemed = 500/100 = 5, os = 0
    // Holdings = 0, asset_value = 0
    assert!((snap.outstanding_shares).abs() < 0.01);
    assert!((snap.asset_value).abs() < 0.01);
}

/// Sell redeems shares correctly: verify outstanding_shares math.
#[tokio::test]
async fn test_sell_redeems_shares() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    // Buy 10 @ $50 = deposit 500, os = 5 (500/100)
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    // Price goes to 80 on day 3
    // NAV on day 3 = 10*80/5 = 160
    // Sell 3 @ $80 on day 4: withdrawal = 240, redeemed = 240/160 = 1.5
    // os = 5 - 1.5 = 3.5
    common::insert_sell_transaction(&db, asset_id, "2025-01-04", 3.0, 80.0, 0.0).await;

    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 80.0, false).await;
    common::insert_daily_price(&db, asset_id, "2025-01-04", 80.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    let snap_d3 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    assert!((snap_d3.nav - 160.0).abs() < 0.01);
    assert!((snap_d3.outstanding_shares - 5.0).abs() < 0.01);

    let snap_d4 = common::get_portfolio_snapshot(&db, "2025-01-04")
        .await
        .unwrap();
    // os = 5 - 1.5 = 3.5, holdings = 7, value = 7*80 = 560
    // NAV = 560/3.5 = 160 (preserved!)
    assert!((snap_d4.outstanding_shares - 3.5).abs() < 0.01);
    assert!((snap_d4.nav - 160.0).abs() < 0.01);
    assert!((snap_d4.asset_value - 560.0).abs() < 0.01);
}

/// 2:1 forward split doubles holdings, NAV stays the same.
#[tokio::test]
async fn test_forward_split_doubles_holdings() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    // Buy 10 shares at $100 on day 1
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 100.0, false).await;

    // 2:1 split on day 2, price halves to $50 (adjusted)
    common::insert_split_transaction(&db, asset_id, "2025-01-03", 2.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 50.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    // Day 1: 10 shares * $100 = $1000, NAV=100, os=10
    let snap_d1 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    assert!((snap_d1.outstanding_shares - 10.0).abs() < 0.01);
    assert!((snap_d1.nav - 100.0).abs() < 0.01);
    assert!((snap_d1.asset_value - 1000.0).abs() < 0.01);

    // Day 2: 20 shares * $50 = $1000, NAV unchanged, os unchanged
    let snap_d2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    assert!((snap_d2.outstanding_shares - 10.0).abs() < 0.01);
    assert!((snap_d2.nav - 100.0).abs() < 0.01);
    assert!((snap_d2.asset_value - 1000.0).abs() < 0.01);

    // Verify asset snapshot shows 20 shares
    let asset_snaps = common::get_asset_snapshots(&db, "2025-01-03").await;
    assert_eq!(asset_snaps.len(), 1);
    assert!((asset_snaps[0].quantity - 20.0).abs() < 0.01);
}

/// 1:4 reverse split quarters holdings, NAV stays the same.
#[tokio::test]
async fn test_reverse_split_quarters_holdings() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    // Buy 100 shares at $10
    common::insert_transaction(&db, asset_id, "2025-01-02", 100.0, 10.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 10.0, false).await;

    // 1:4 reverse split on day 2, price quadruples to $40
    common::insert_split_transaction(&db, asset_id, "2025-01-03", 0.25).await;
    common::insert_daily_price(&db, asset_id, "2025-01-03", 40.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    // Day 1: 100 * $10 = $1000
    let snap_d1 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    assert!((snap_d1.asset_value - 1000.0).abs() < 0.01);

    // Day 2: 25 * $40 = $1000, NAV unchanged
    let snap_d2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    assert!((snap_d2.outstanding_shares - snap_d1.outstanding_shares).abs() < 0.01);
    assert!((snap_d2.nav - 100.0).abs() < 0.01);
    assert!((snap_d2.asset_value - 1000.0).abs() < 0.01);

    let asset_snaps = common::get_asset_snapshots(&db, "2025-01-03").await;
    assert!((asset_snaps[0].quantity - 25.0).abs() < 0.01);
}

/// Split mid-history: buy, price rises, split, verify NAV continuity.
#[tokio::test]
async fn test_split_mid_history_nav_continuity() {
    let db = common::setup_test_db().await;
    let mock = common::MockMarketDataSources::new();

    let asset_id = common::insert_asset(&db, "XFAKE1", "Test Stock", "stock", "EUR").await;
    // Buy 10 shares at $50
    common::insert_transaction(&db, asset_id, "2025-01-02", 10.0, 50.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-02", 50.0, false).await;

    // Price rises to $80 on day 2
    common::insert_daily_price(&db, asset_id, "2025-01-03", 80.0, false).await;

    // 2:1 split on day 3, adjusted price = $40
    common::insert_split_transaction(&db, asset_id, "2025-01-06", 2.0).await;
    common::insert_daily_price(&db, asset_id, "2025-01-06", 40.0, false).await;

    // Price continues to $45 on day 4
    common::insert_daily_price(&db, asset_id, "2025-01-07", 45.0, false).await;

    nav::ensure_portfolio_history(&db, &nav_market_data(&mock))
        .await
        .unwrap();

    // Day 1: 10 * $50 = $500, NAV=100, os=5
    let snap_d1 = common::get_portfolio_snapshot(&db, "2025-01-02")
        .await
        .unwrap();
    assert!((snap_d1.nav - 100.0).abs() < 0.01);

    // Day 2: 10 * $80 = $800, NAV = 800/5 = 160
    let snap_d2 = common::get_portfolio_snapshot(&db, "2025-01-03")
        .await
        .unwrap();
    assert!((snap_d2.nav - 160.0).abs() < 0.01);

    // Day 3 (split): 20 * $40 = $800, NAV = 800/5 = 160 (unchanged)
    let snap_d3 = common::get_portfolio_snapshot(&db, "2025-01-06")
        .await
        .unwrap();
    assert!((snap_d3.nav - 160.0).abs() < 0.01);
    assert!((snap_d3.outstanding_shares - 5.0).abs() < 0.01);

    // Day 4: 20 * $45 = $900, NAV = 900/5 = 180
    let snap_d4 = common::get_portfolio_snapshot(&db, "2025-01-07")
        .await
        .unwrap();
    assert!((snap_d4.nav - 180.0).abs() < 0.01);
}
