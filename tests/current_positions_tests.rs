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

#[tokio::test]
async fn focused_current_positions_report_remaining_cost_dividends_and_open_gain_separately() {
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_asset(&db, "XFAKECUR4", "Financial Facts Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 1.0).await;
    common::insert_transaction(&db, asset_id, "2025-06-02", 4.0, 14.0, 2.0).await;
    common::insert_split_transaction(&db, asset_id, "2025-06-03", 2.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-04", 10.0, 1.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-06-05", 3.0, 20.0, 0.0).await;
    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert("XFAKECUR4".to_owned(), vec![("2025-06-10".to_owned(), 8.0)]);
    let market_data = common::market_data_at(&sources, fixed_today());
    let result = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();
    let position = &result.positions[0];

    // Split doubles units without changing total cost; the sell removes 25% of it.
    assert!((position.total_qty - 9.0).abs() < 1e-9);
    assert!((position.total_invested.unwrap() - 59.25).abs() < 1e-9);
    assert!((position.avg_cost.unwrap() - 6.5833333333).abs() < 1e-9);
    assert!((position.dividends_received.unwrap() - 9.0).abs() < 1e-9);
    assert!((position.current_value.unwrap() - 72.0).abs() < 1e-9);
    assert!((position.open_position_gain_loss.unwrap() - 12.75).abs() < 1e-9);
    assert!((result.total_dividends.unwrap() - 9.0).abs() < 1e-9);
    assert!((result.total_open_position_gain_loss.unwrap() - 12.75).abs() < 1e-9);
}

#[tokio::test]
async fn focused_current_positions_keep_sell_and_dividend_facts_separate_from_monetary_aggregates()
{
    let db = common::setup_test_db().await;
    let stock_id = common::insert_asset(&db, "XFAKECUR4", "Current Stock", "stock", "EUR").await;
    let monetary_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKECUR5",
        "Current Monetary Fund",
        "EUR",
        "F000CURRENT2",
    )
    .await;
    common::insert_transaction(&db, stock_id, "2025-06-02", 10.0, 10.0, 0.0).await;
    common::insert_dividend_transaction(&db, stock_id, "2025-06-03", 5.0, 0.0).await;
    common::insert_sell_transaction(&db, stock_id, "2025-06-04", 4.0, 12.0, 0.0).await;
    common::insert_transaction(&db, monetary_id, "2025-06-02", 10.0, 100.0, 0.0).await;
    common::insert_dividend_transaction(&db, monetary_id, "2025-06-03", 20.0, 0.0).await;
    common::insert_sell_transaction(&db, monetary_id, "2025-06-04", 4.0, 105.0, 0.0).await;

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKECUR4".to_owned(),
        vec![("2025-06-09".to_owned(), 12.0)],
    );
    sources.historical_prices.insert(
        "F000CURRENT2".to_owned(),
        vec![("2025-06-09".to_owned(), 105.0)],
    );
    let market_data = common::market_data_at(&sources, fixed_today());

    let result = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();

    let position = &result.positions[0];
    assert!((position.total_qty - 6.0).abs() < 1e-9);
    assert!((position.total_invested.unwrap() - 60.0).abs() < 1e-9);
    assert!((position.dividends_received.unwrap() - 5.0).abs() < 1e-9);
    assert!((position.open_position_gain_loss.unwrap() - 12.0).abs() < 1e-9);
    let monetary_position = &result.monetary_positions[0];
    assert!((monetary_position.total_qty - 6.0).abs() < 1e-9);
    assert!((monetary_position.total_invested.unwrap() - 600.0).abs() < 1e-9);
    assert!((monetary_position.dividends_received.unwrap() - 20.0).abs() < 1e-9);
    assert!((monetary_position.open_position_gain_loss.unwrap() - 30.0).abs() < 1e-9);
    assert!((result.total_current_value.unwrap() - 72.0).abs() < 1e-9);
    assert!((result.total_monetary_value.unwrap() - 630.0).abs() < 1e-9);
    assert!((result.total_value.unwrap() - 702.0).abs() < 1e-9);
    assert!((result.total_invested.unwrap() - 60.0).abs() < 1e-9);
    assert!((result.total_dividends.unwrap() - 5.0).abs() < 1e-9);
    assert!((result.total_open_position_gain_loss.unwrap() - 12.0).abs() < 1e-9);
    assert!((result.total_monetary_invested.unwrap() - 600.0).abs() < 1e-9);
    assert!((result.total_monetary_dividends.unwrap() - 20.0).abs() < 1e-9);
    assert!((result.total_monetary_open_position_gain_loss.unwrap() - 30.0).abs() < 1e-9);
}

#[tokio::test]
async fn focused_current_positions_apply_identical_financial_facts_to_performance_and_monetary_holdings(
) {
    let db = common::setup_test_db().await;
    let stock_id =
        common::insert_asset(&db, "XFAKECUR6", "Performance Stock", "stock", "EUR").await;
    let monetary_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKECUR7",
        "Monetary Fund",
        "EUR",
        "F000CURRENT3",
    )
    .await;
    for asset_id in [stock_id, monetary_id] {
        common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 1.0).await;
        common::insert_transaction(&db, asset_id, "2025-06-02", 4.0, 14.0, 2.0).await;
        common::insert_split_transaction(&db, asset_id, "2025-06-03", 2.0).await;
        common::insert_dividend_transaction(&db, asset_id, "2025-06-04", 10.0, 1.0).await;
        common::insert_sell_transaction(&db, asset_id, "2025-06-05", 3.0, 20.0, 0.0).await;
    }

    let mut sources = common::MockMarketDataSources::new();
    sources
        .historical_prices
        .insert("XFAKECUR6".to_owned(), vec![("2025-06-09".to_owned(), 8.0)]);
    sources.historical_prices.insert(
        "F000CURRENT3".to_owned(),
        vec![("2025-06-09".to_owned(), 8.0)],
    );
    let market_data = common::market_data_at(&sources, fixed_today());

    let result = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();
    let performance = &result.positions[0];
    let monetary = &result.monetary_positions[0];

    assert!((performance.total_qty - 9.0).abs() < 1e-9);
    assert_eq!(performance.total_invested, monetary.total_invested);
    assert_eq!(performance.avg_cost, monetary.avg_cost);
    assert_eq!(performance.dividends_received, monetary.dividends_received);
    assert_eq!(performance.current_value, monetary.current_value);
    assert_eq!(
        performance.open_position_gain_loss,
        monetary.open_position_gain_loss
    );
    assert_eq!(
        performance.open_position_gain_loss_pct,
        monetary.open_position_gain_loss_pct
    );
}

#[tokio::test]
async fn focused_current_positions_never_uses_later_fx_for_historical_ledger_facts() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEFX1", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-06-02", 2.0, 10.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-03", 3.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 12.0, false).await;
    // This rate supports the Individual price but must not be used for earlier ledger entries.
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-09", 0.9).await;

    let result = portfolio::get_current_positions(
        &db,
        &common::market_data_at(&common::MockMarketDataSources::new(), fixed_today()),
    )
    .await
    .unwrap();
    let position = &result.positions[0];

    assert_eq!(position.total_qty, 2.0);
    assert_eq!(position.current_value, Some(21.6));
    assert!(position.total_invested.is_none());
    assert!(position.avg_cost.is_none());
    assert!(position.dividends_received.is_none());
    assert!(position.open_position_gain_loss.is_none());
    assert!(result.total_current_value.is_some());
    assert!(result.total_invested.is_none());
    assert!(result.total_dividends.is_none());
    assert!(result.total_open_position_gain_loss.is_none());
    assert!(result.total_monetary_invested.is_some());
    assert!(position
        .market_data_limitations
        .iter()
        .any(|limitation| matches!(
            limitation.subject,
            rstock::models::MarketDataSubject::FxRate { ref currency } if currency == "USD"
        )));
}

#[tokio::test]
async fn focused_current_positions_use_latest_fx_on_or_before_each_transaction_date() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEFX2", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-06-02", 2.0, 10.0, 1.0).await;
    common::insert_transaction(&db, asset_id, "2025-06-04", 1.0, 20.0, 2.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-04", 10.0, 1.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 15.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-01", 0.8).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-03", 0.9).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-05", 1.2).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-09", 1.0).await;

    let result = portfolio::get_current_positions(
        &db,
        &common::market_data_at(&common::MockMarketDataSources::new(), fixed_today()),
    )
    .await
    .unwrap();
    let position = &result.positions[0];

    // Buy costs are 21 * 0.8 and 22 * 0.9; the dividend is (10 - 1) * 0.9.
    assert!((position.total_invested.unwrap() - 36.6).abs() < 1e-9);
    assert!((position.avg_cost.unwrap() - 12.2).abs() < 1e-9);
    assert!((position.dividends_received.unwrap() - 8.1).abs() < 1e-9);
    assert!((position.current_value.unwrap() - 45.0).abs() < 1e-9);
    assert!((position.open_position_gain_loss.unwrap() - 8.4).abs() < 1e-9);
    assert!((result.total_invested.unwrap() - 36.6).abs() < 1e-9);
    assert!((result.total_dividends.unwrap() - 8.1).abs() < 1e-9);
    assert!((result.total_open_position_gain_loss.unwrap() - 8.4).abs() < 1e-9);
}

#[tokio::test]
async fn focused_current_positions_are_identical_across_requests_when_fx_is_source_available_but_uncached(
) {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEFX10", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-03", 3.0, 0.0).await;

    let mut sources = common::MockMarketDataSources::new();
    sources.historical_prices.insert(
        "XFAKEFX10".to_owned(),
        vec![("2025-06-05".to_owned(), 12.0)],
    );
    sources.exchange_rates.insert(
        "USDEUR".to_owned(),
        vec![
            ("2025-06-01".to_owned(), 0.8),
            ("2025-06-05".to_owned(), 0.9),
        ],
    );
    let market_data = common::market_data_at(&sources, fixed_today());

    let first = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();
    let second = portfolio::get_current_positions(&db, &market_data)
        .await
        .unwrap();

    assert_eq!(first.positions.len(), 1);
    let first_position = &first.positions[0];
    let second_position = &second.positions[0];
    // Cost and dividend facts must be complete on the first request already.
    assert!((first_position.total_invested.unwrap() - 16.0).abs() < 1e-9);
    assert!((first_position.avg_cost.unwrap() - 8.0).abs() < 1e-9);
    assert!((first_position.dividends_received.unwrap() - 2.4).abs() < 1e-9);
    assert_eq!(first_position.total_qty, second_position.total_qty);
    assert_eq!(
        first_position.total_invested,
        second_position.total_invested
    );
    assert_eq!(first_position.avg_cost, second_position.avg_cost);
    assert_eq!(
        first_position.dividends_received,
        second_position.dividends_received
    );
    assert_eq!(first_position.current_value, second_position.current_value);
    assert!(first_position.market_data_limitations.is_empty());
    assert!((first.total_invested.unwrap() - 16.0).abs() < 1e-9);
    assert!((first.total_dividends.unwrap() - 2.4).abs() < 1e-9);
    assert!((second.total_invested.unwrap() - 16.0).abs() < 1e-9);
    assert!((second.total_dividends.unwrap() - 2.4).abs() < 1e-9);
    assert_eq!(first.total_value, second.total_value);
}

#[tokio::test]
async fn focused_current_positions_keep_independent_facts_when_buy_fx_is_missing() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEFX3", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-02", 3.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 12.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-02", 0.8).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-09", 0.9).await;

    let result = portfolio::get_current_positions(
        &db,
        &common::market_data_at(&common::MockMarketDataSources::new(), fixed_today()),
    )
    .await
    .unwrap();
    let position = &result.positions[0];

    assert_eq!(position.total_qty, 2.0);
    assert_eq!(position.total_invested, None);
    assert_eq!(position.avg_cost, None);
    assert_eq!(position.current_value, Some(21.6));
    assert_eq!(position.dividends_received, Some(2.4));
    assert!(position.open_position_gain_loss.is_none());
    assert!(position.open_position_gain_loss_pct.is_none());
    assert_eq!(result.total_current_value, Some(21.6));
    assert_eq!(result.total_invested, None);
    assert_eq!(result.total_dividends, Some(2.4));
    assert!(result.total_open_position_gain_loss.is_none());
    assert!(result.total_open_position_gain_loss_pct.is_none());
    assert_eq!(result.total_value, Some(21.6));
    assert!(position
        .market_data_limitations
        .iter()
        .any(|limitation| matches!(
            limitation.subject,
            rstock::models::MarketDataSubject::FxRate { ref currency } if currency == "USD"
        )));
}

#[tokio::test]
async fn focused_current_positions_keep_quantity_and_current_value_when_dividend_fx_is_missing() {
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_asset(&db, "XFAKEFXDIV", "USD Dividend Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-02", 3.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 12.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-09", 0.9).await;

    let result = portfolio::get_current_positions(
        &db,
        &common::market_data_at(&common::MockMarketDataSources::new(), fixed_today()),
    )
    .await
    .unwrap();
    let position = &result.positions[0];

    assert_eq!(position.total_qty, 2.0);
    assert_eq!(position.total_invested, None);
    assert_eq!(position.dividends_received, None);
    assert_eq!(position.current_value, Some(21.6));
    assert!(position.open_position_gain_loss.is_none());
    assert!(result.total_dividends.is_none());
}

#[tokio::test]
async fn focused_current_positions_reopen_after_liquidation_with_fresh_cost_basis() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEREOPEN", "Reopened Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 1.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-06-02", 2.0, 12.0, 3.0).await;
    common::insert_transaction(&db, asset_id, "2025-06-03", 1.0, 7.0, 0.5).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 8.0, false).await;

    let result = portfolio::get_current_positions(
        &db,
        &common::market_data_at(&common::MockMarketDataSources::new(), fixed_today()),
    )
    .await
    .unwrap();
    let position = &result.positions[0];

    assert_eq!(position.total_qty, 1.0);
    assert_eq!(position.total_invested, Some(7.5));
    assert_eq!(position.avg_cost, Some(7.5));
    assert_eq!(position.current_value, Some(8.0));
    assert_eq!(position.open_position_gain_loss, Some(0.5));
}
