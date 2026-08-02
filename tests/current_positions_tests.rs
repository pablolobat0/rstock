mod common;

use chrono::NaiveDate;
use rstock::db::repos::portfolio_history_repo;
use rstock::services::portfolio;

fn fixed_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()
}

#[tokio::test]
async fn focused_current_positions_returns_empty_inventory_without_nav_history() {
    let db = common::setup_test_db().await;
    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());

    let result = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();

    assert!(result.positions.is_empty());
    assert!(result.monetary_positions.is_empty());
    assert_eq!(result.total_value, Some(0.0));
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn focused_current_positions_uses_fixed_date_and_shared_ledger_projection() {
    let db = common::setup_test_db().await;
    let stock_id = common::insert_asset(&db, "XFAKECUR1", "Current Stock", "stock", "EUR").await;
    let monetary_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKECUR2",
        "Current Monetary Fund",
        "EUR",
        "F000CURRENT",
    )
    .await;
    common::insert_transaction(&db, stock_id, "2025-06-02", 2.0, 10.0, 1.0).await;
    common::insert_split_transaction(&db, stock_id, "2025-06-03", 2.0).await;
    common::insert_transaction(&db, stock_id, "2025-06-11", 5.0, 10.0, 0.0).await;
    common::insert_transaction(&db, monetary_id, "2025-06-02", 3.0, 100.0, 0.0).await;
    common::insert_transaction(&db, monetary_id, "2025-06-11", 4.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, monetary_id, "2025-06-09", 101.0, false).await;

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKECUR1".to_owned(),
        vec![("2025-06-10".to_owned(), 12.0)],
    );
    let market_data = common::market_data_at(&sources, fixed_today());
    let result = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();

    assert_eq!(result.positions.len(), 1);
    assert_eq!(result.monetary_positions.len(), 1);
    assert!((result.positions[0].total_qty - 4.0).abs() < 1e-9);
    assert!((result.positions[0].total_invested.unwrap() - 21.0).abs() < 1e-9);
    assert!((result.monetary_positions[0].total_qty - 3.0).abs() < 1e-9);
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn focused_current_positions_include_buy_after_effective_valuation_date() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKECUR3", "New Stock", "stock", "EUR").await;
    common::insert_portfolio_snapshot(&db, "2025-06-05", 100.0, 1.0).await;
    common::insert_transaction(&db, asset_id, "2025-06-10", 2.0, 10.0, 0.0).await;

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKECUR3".to_owned(),
        vec![("2025-06-10".to_owned(), 12.0)],
    );
    let market_data = common::market_data_at(&sources, fixed_today());
    let result = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();

    assert_eq!(result.positions.len(), 1);
    assert_eq!(result.positions[0].ticker, "XFAKECUR3");
    assert!((result.positions[0].total_qty - 2.0).abs() < 1e-9);
    assert_eq!(
        portfolio_history_repo::find_latest(&db)
            .await
            .unwrap()
            .unwrap()
            .date,
        "2025-06-05"
    );
}

#[tokio::test]
async fn focused_current_positions_apply_fixed_clock_individual_price_semantics() {
    let db = common::setup_test_db().await;
    let live_etf_id = common::insert_etf_asset(&db, "XFAKEETF3", "Live ETF", "EUR", "ETF3").await;
    let fallback_etf_id =
        common::insert_etf_asset(&db, "XFAKEETF4", "Fallback ETF", "EUR", "ETF4").await;
    let fund_id = common::insert_fund_asset(&db, "XFAKEF3", "Fund", "EUR", "FUND3").await;
    let stock_id = common::insert_asset(&db, "XFAKES6", "Stock", "stock", "EUR").await;

    for asset_id in [live_etf_id, fallback_etf_id, fund_id, stock_id] {
        common::insert_transaction(&db, asset_id, "2025-06-02", 1.0, 90.0, 0.0).await;
        common::insert_daily_price(&db, asset_id, "2025-06-09", 100.0, false).await;
    }

    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert("ETF3".to_owned(), vec![("2025-06-10".to_owned(), 125.0)]);
    sources
        .historical_prices
        .insert("ETF4".to_owned(), vec![("2025-06-08".to_owned(), 999.0)]);
    sources
        .historical_prices
        .insert("FUND3".to_owned(), vec![("2025-06-10".to_owned(), 130.0)]);
    sources
        .historical_prices
        .insert("XFAKES6".to_owned(), vec![("2025-06-10".to_owned(), 126.0)]);

    let result =
        portfolio::get_current_positions(&db, &common::market_data_at(&sources, fixed_today()))
            .await
            .unwrap();
    let position = |ticker: &str| {
        result
            .positions
            .iter()
            .find(|position| position.ticker == ticker)
            .unwrap()
    };

    assert_eq!(position("XFAKEETF3").current_price, Some(125.0));
    assert_eq!(
        position("XFAKEETF3").price_date.as_deref(),
        Some("2025-06-10")
    );
    assert_eq!(position("XFAKEETF4").current_price, Some(100.0));
    assert_eq!(
        position("XFAKEETF4").price_date.as_deref(),
        Some("2025-06-09")
    );
    assert_eq!(position("XFAKEF3").current_price, Some(100.0));
    assert_eq!(
        position("XFAKEF3").price_date.as_deref(),
        Some("2025-06-09")
    );
    assert_eq!(position("XFAKES6").current_price, Some(126.0));
    assert_eq!(
        position("XFAKES6").price_date.as_deref(),
        Some("2025-06-10")
    );
}
