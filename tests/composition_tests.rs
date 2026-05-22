mod common;

use rstock::models::StockInfo;
use rstock::services::composition::compute_composition;
use rstock::services::market_data::MarketData;
use sea_orm::{EntityTrait, Set};

use common::{
    insert_daily_price, insert_portfolio_snapshot, insert_transaction, setup_test_db,
    MockPriceFetcher,
};
use rstock::db::entities::{asset, portfolio_asset_history};

async fn insert_classified_asset(
    db: &sea_orm::DatabaseConnection,
    ticker: &str,
    name: &str,
    asset_type: &str,
    currency: &str,
    asset_class: Option<&str>,
    equity_style: Option<&str>,
    management: Option<&str>,
) -> i32 {
    let record = asset::ActiveModel {
        ticker: Set(ticker.to_owned()),
        name: Set(name.to_owned()),
        asset_type: Set(asset_type.to_owned()),
        currency: Set(currency.to_owned()),
        created_at: Set("2025-01-01T00:00:00".to_owned()),
        asset_class: Set(asset_class.map(str::to_owned)),
        equity_style: Set(equity_style.map(str::to_owned)),
        management: Set(management.map(str::to_owned)),
        ..Default::default()
    };
    let result = asset::Entity::insert(record)
        .exec(db)
        .await
        .expect("failed to insert classified asset");
    result.last_insert_id
}

async fn insert_asset_snapshot(
    db: &sea_orm::DatabaseConnection,
    date: &str,
    asset_id: i32,
    quantity: f64,
    closing_price: f64,
    market_value: f64,
) {
    let record = portfolio_asset_history::ActiveModel {
        date: Set(date.to_owned()),
        asset_id: Set(asset_id),
        quantity: Set(quantity),
        closing_price: Set(closing_price),
        market_value: Set(market_value),
        exchange_rate: Set(1.0),
        ..Default::default()
    };
    portfolio_asset_history::Entity::insert(record)
        .exec(db)
        .await
        .expect("failed to insert asset snapshot");
}

fn mock_stock_info(
    ticker: &str,
    sector: Option<&str>,
    industry: Option<&str>,
    country: Option<&str>,
    market_cap: Option<f64>,
) -> StockInfo {
    StockInfo {
        ticker: ticker.to_owned(),
        name: Some(format!("{ticker} Inc")),
        currency: Some("USD".to_owned()),
        current_price: Some(100.0),
        previous_close: Some(99.0),
        day_range: None,
        fifty_two_week_range: None,
        volume: None,
        avg_volume: None,
        market_cap,
        pe_ttm: None,
        eps_ttm: None,
        dividend_yield: None,
        sector: sector.map(str::to_owned),
        industry: industry.map(str::to_owned),
        country: country.map(str::to_owned),
    }
}

fn market_data_from(fetcher: MockPriceFetcher) -> MarketData {
    MarketData::new(Box::new(fetcher))
}

#[tokio::test]
async fn test_composition_direct_stocks_only() {
    let db = setup_test_db().await;

    let id1 = insert_classified_asset(
        &db,
        "XFAKE1",
        "TechCo",
        "stock",
        "EUR",
        Some("equity"),
        None,
        None,
    )
    .await;
    let id2 = insert_classified_asset(
        &db,
        "XFAKE2",
        "HealthCo",
        "stock",
        "EUR",
        Some("equity"),
        None,
        None,
    )
    .await;

    // Insert transactions
    insert_transaction(&db, id1, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_transaction(&db, id2, "2025-01-02", 5.0, 200.0, 0.0).await;

    // Insert prices
    insert_daily_price(&db, id1, "2025-01-02", 100.0, false).await;
    insert_daily_price(&db, id2, "2025-01-02", 200.0, false).await;

    // Insert portfolio snapshot (needed for get_asset_positions)
    insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 20.0).await;
    insert_asset_snapshot(&db, "2025-01-02", id1, 10.0, 100.0, 1000.0).await;
    insert_asset_snapshot(&db, "2025-01-02", id2, 5.0, 200.0, 1000.0).await;

    let mut fetcher = MockPriceFetcher::new();
    fetcher
        .historical_prices
        .insert("XFAKE1".to_owned(), vec![("2025-01-02".to_owned(), 100.0)]);
    fetcher
        .historical_prices
        .insert("XFAKE2".to_owned(), vec![("2025-01-02".to_owned(), 200.0)]);
    fetcher.stock_info.insert(
        "XFAKE1".to_owned(),
        mock_stock_info(
            "XFAKE1",
            Some("Technology"),
            Some("Semiconductors"),
            Some("United States"),
            Some(50_000_000_000.0),
        ),
    );
    fetcher.stock_info.insert(
        "XFAKE2".to_owned(),
        mock_stock_info(
            "XFAKE2",
            Some("Healthcare"),
            Some("Pharma"),
            Some("Germany"),
            Some(5_000_000_000.0),
        ),
    );

    let market_data = market_data_from(fetcher);
    let result = compute_composition(&db, &market_data).await.unwrap();

    // Asset class breakdown: both are equity
    assert_eq!(result.asset_class_breakdown.len(), 1);
    assert_eq!(result.asset_class_breakdown[0].label, "equity");

    // Sector breakdown: Technology and Healthcare
    assert_eq!(result.sector_breakdown.len(), 2);

    // Country breakdown: US and Germany
    assert_eq!(result.country_breakdown.len(), 2);

    // Market cap: one large (50B), one mid (5B)
    assert_eq!(result.market_cap_breakdown.len(), 2);
}

#[tokio::test]
async fn test_composition_unclassified_asset() {
    let db = setup_test_db().await;

    let id1 =
        insert_classified_asset(&db, "XFAKE1", "UnknownCo", "stock", "EUR", None, None, None).await;

    insert_transaction(&db, id1, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, id1, "2025-01-02", 100.0, false).await;
    insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 10.0).await;
    insert_asset_snapshot(&db, "2025-01-02", id1, 10.0, 100.0, 1000.0).await;

    let mut fetcher = MockPriceFetcher::new();
    fetcher
        .historical_prices
        .insert("XFAKE1".to_owned(), vec![("2025-01-02".to_owned(), 100.0)]);
    fetcher.stock_info.insert(
        "XFAKE1".to_owned(),
        mock_stock_info("XFAKE1", Some("Technology"), None, Some("US"), None),
    );

    let market_data = market_data_from(fetcher);
    let result = compute_composition(&db, &market_data).await.unwrap();

    // Unclassified asset
    assert_eq!(result.asset_class_breakdown.len(), 1);
    assert_eq!(result.asset_class_breakdown[0].label, "Unclassified");
}

#[tokio::test]
async fn test_composition_empty_portfolio() {
    let db = setup_test_db().await;
    let market_data = market_data_from(MockPriceFetcher::new());

    let result = compute_composition(&db, &market_data).await.unwrap();

    assert!(result.asset_class_breakdown.is_empty());
    assert!(!result.warnings.is_empty());
}

#[tokio::test]
async fn test_composition_failed_stock_info() {
    let db = setup_test_db().await;

    let id1 = insert_classified_asset(
        &db,
        "XFAKE1",
        "FailCo",
        "stock",
        "EUR",
        Some("equity"),
        None,
        None,
    )
    .await;

    insert_transaction(&db, id1, "2025-01-02", 10.0, 100.0, 0.0).await;
    insert_daily_price(&db, id1, "2025-01-02", 100.0, false).await;
    insert_portfolio_snapshot(&db, "2025-01-02", 100.0, 10.0).await;
    insert_asset_snapshot(&db, "2025-01-02", id1, 10.0, 100.0, 1000.0).await;

    let mut fetcher = MockPriceFetcher::new();
    fetcher
        .historical_prices
        .insert("XFAKE1".to_owned(), vec![("2025-01-02".to_owned(), 100.0)]);
    // No stock_info for XFAKE1 -> get_stock_info will fail

    let market_data = market_data_from(fetcher);
    let result = compute_composition(&db, &market_data).await.unwrap();

    // Should have a warning about failed lookup
    assert!(result.warnings.iter().any(|w| w.contains("XFAKE1")));
}
