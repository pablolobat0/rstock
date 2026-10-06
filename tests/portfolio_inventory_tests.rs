#![allow(clippy::float_cmp)]

pub mod common;

use chrono::NaiveDate;
use rstock::db::repos::portfolio_history_repo;
use rstock::models::{
    FactAvailability, MarketDataSubject, PortfolioInventory, PortfolioInventorySection,
};
use rstock::services::{composition, portfolio_inventory};

fn fixed_today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()
}

fn performance(inventory: &PortfolioInventory) -> &PortfolioInventorySection {
    &inventory.performance_holdings
}

#[tokio::test]
async fn inventory_returns_empty_sections_with_known_zero_aggregates() {
    let db = common::setup_test_db().await;
    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());

    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert!(inventory.performance_holdings.positions.is_empty());
    assert!(inventory.monetary_holdings.positions.is_empty());
    let empty_aggregates = &inventory.performance_holdings.aggregates;
    assert_eq!(
        empty_aggregates.current_value,
        FactAvailability::Available(0.0)
    );
    assert_eq!(
        empty_aggregates.invested_cost,
        FactAvailability::Available(0.0)
    );
    assert_eq!(empty_aggregates.dividends, FactAvailability::Available(0.0));
    assert_eq!(
        empty_aggregates.open_position_gain_loss,
        FactAvailability::Available(0.0)
    );
    assert_eq!(
        empty_aggregates.open_position_gain_loss_pct,
        FactAvailability::Available(0.0)
    );
    assert_eq!(
        inventory.monetary_holdings.aggregates.current_value,
        FactAvailability::Available(0.0)
    );
    assert_eq!(inventory.total_value, FactAvailability::Available(0.0));
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn inventory_returns_empty_sections_for_future_only_transactions() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEFUT1", "Future Stock", "stock", "EUR").await;
    let monetary_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKEFUT2",
        "Future Monetary Fund",
        "EUR",
        "F000FUTURE",
    )
    .await;
    common::insert_transaction(&db, asset_id, "2025-06-11", 2.0, 10.0, 0.0).await;
    common::insert_transaction(&db, monetary_id, "2025-06-11", 2.0, 10.0, 0.0).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert!(inventory.performance_holdings.positions.is_empty());
    assert!(inventory.monetary_holdings.positions.is_empty());
    assert_eq!(inventory.total_value, FactAvailability::Available(0.0));
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn inventory_uses_fixed_date_and_shared_ledger_projection() {
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
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert_eq!(inventory.performance_holdings.positions.len(), 1);
    assert_eq!(inventory.monetary_holdings.positions.len(), 1);
    let stock = &inventory.performance_holdings.positions[0];
    assert!((stock.quantity - 4.0).abs() < 1e-9);
    assert_eq!(stock.invested_cost.value().copied().unwrap(), 21.0);
    let monetary = &inventory.monetary_holdings.positions[0];
    assert!((monetary.quantity - 3.0).abs() < 1e-9);
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn inventory_includes_buy_after_effective_valuation_date() {
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
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert_eq!(inventory.performance_holdings.positions.len(), 1);
    let position = &inventory.performance_holdings.positions[0];
    assert_eq!(position.ticker, "XFAKECUR3");
    assert!((position.quantity - 2.0).abs() < 1e-9);
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
async fn inventory_applies_fixed_clock_individual_price_semantics() {
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
    let market_data = common::market_data_at(&sources, fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let position = |ticker: &str| {
        inventory
            .performance_holdings
            .positions
            .iter()
            .find(|position| position.ticker == ticker)
            .unwrap()
    };
    let price_of = |position: &rstock::models::InventoryPosition| {
        position.individual_price.value().cloned().unwrap()
    };
    assert_eq!(price_of(position("XFAKEETF3")).native_price, 125.0);
    assert_eq!(price_of(position("XFAKEETF3")).price_date, "2025-06-10");
    assert_eq!(price_of(position("XFAKEETF4")).native_price, 100.0);
    assert_eq!(price_of(position("XFAKEETF4")).price_date, "2025-06-09");
    assert_eq!(price_of(position("XFAKEF3")).native_price, 100.0);
    assert_eq!(price_of(position("XFAKEF3")).price_date, "2025-06-09");
    assert_eq!(price_of(position("XFAKES6")).native_price, 126.0);
    assert_eq!(price_of(position("XFAKES6")).price_date, "2025-06-10");
}

#[tokio::test]
async fn inventory_reports_remaining_cost_dividends_and_open_gain_separately() {
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
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let position = &performance(&inventory).positions[0];

    // Split doubles units without changing total cost; the sell removes 25% of it.
    assert!((position.quantity - 9.0).abs() < 1e-9);
    assert!((position.invested_cost.value().copied().unwrap() - 59.25).abs() < 1e-9);
    assert!((position.average_cost.value().copied().unwrap() - 6.583_333_333_3).abs() < 1e-9);
    assert!((position.dividends.value().copied().unwrap() - 9.0).abs() < 1e-9);
    assert!((position.current_value.value().copied().unwrap() - 72.0).abs() < 1e-9);
    assert!((position.open_position_gain_loss.value().copied().unwrap() - 12.75).abs() < 1e-9);
    let aggregates = &performance(&inventory).aggregates;
    assert!((aggregates.dividends.value().copied().unwrap() - 9.0).abs() < 1e-9);
    assert!((aggregates.open_position_gain_loss.value().copied().unwrap() - 12.75).abs() < 1e-9);
}

#[tokio::test]
async fn inventory_keeps_sell_and_dividend_facts_separate_from_monetary_aggregates() {
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

    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    let position = &performance(&inventory).positions[0];
    assert!((position.quantity - 6.0).abs() < 1e-9);
    assert!((position.invested_cost.value().copied().unwrap() - 60.0).abs() < 1e-9);
    assert!((position.dividends.value().copied().unwrap() - 5.0).abs() < 1e-9);
    assert!((position.open_position_gain_loss.value().copied().unwrap() - 12.0).abs() < 1e-9);
    let monetary_position = &inventory.monetary_holdings.positions[0];
    assert!((monetary_position.quantity - 6.0).abs() < 1e-9);
    assert!((monetary_position.invested_cost.value().copied().unwrap() - 600.0).abs() < 1e-9);
    assert!((monetary_position.dividends.value().copied().unwrap() - 20.0).abs() < 1e-9);
    assert!(
        (monetary_position
            .open_position_gain_loss
            .value()
            .copied()
            .unwrap()
            - 30.0)
            .abs()
            < 1e-9
    );
    assert_section_aggregates(
        performance(&inventory),
        Some(72.0),
        Some(60.0),
        Some(5.0),
        Some(12.0),
    );
    assert_section_aggregates(
        &inventory.monetary_holdings,
        Some(630.0),
        Some(600.0),
        Some(20.0),
        Some(30.0),
    );
    assert_eq!(inventory.total_value.value().copied(), Some(702.0));
}

/// Asserts a section's ordinary aggregates are exactly the expected values;
/// `None` means the aggregate must be unavailable, not a partial sum.
fn assert_section_aggregates(
    section: &PortfolioInventorySection,
    current_value: Option<f64>,
    invested_cost: Option<f64>,
    dividends: Option<f64>,
    open_position_gain_loss: Option<f64>,
) {
    let aggregates = &section.aggregates;
    let fact_value = |fact: &FactAvailability<f64>| fact.value().copied();
    assert_eq!(fact_value(&aggregates.current_value), current_value);
    assert_eq!(fact_value(&aggregates.invested_cost), invested_cost);
    assert_eq!(fact_value(&aggregates.dividends), dividends);
    assert_eq!(
        fact_value(&aggregates.open_position_gain_loss),
        open_position_gain_loss
    );
}

#[tokio::test]
async fn inventory_applies_identical_financial_facts_to_performance_and_monetary_holdings() {
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

    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let performance_position = &performance(&inventory).positions[0];
    let monetary = &inventory.monetary_holdings.positions[0];

    assert!((performance_position.quantity - 9.0).abs() < 1e-9);
    assert_eq!(performance_position.invested_cost, monetary.invested_cost);
    assert_eq!(performance_position.average_cost, monetary.average_cost);
    assert_eq!(performance_position.dividends, monetary.dividends);
    assert_eq!(performance_position.current_value, monetary.current_value);
    assert_eq!(
        performance_position.open_position_gain_loss,
        monetary.open_position_gain_loss
    );
    assert_eq!(
        performance_position.open_position_gain_loss_pct,
        monetary.open_position_gain_loss_pct
    );
}

#[tokio::test]
async fn inventory_never_uses_later_fx_for_historical_ledger_facts() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEFX1", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-06-02", 2.0, 10.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-03", 3.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 12.0, false).await;
    // This rate supports the Individual price but must not be used for earlier ledger entries.
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-09", 0.9).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let position = &performance(&inventory).positions[0];

    assert_eq!(position.quantity, 2.0);
    assert_eq!(position.current_value.value().copied(), Some(21.6));
    assert_eq!(position.invested_cost, FactAvailability::Unavailable);
    assert_eq!(position.average_cost, FactAvailability::Unavailable);
    assert_eq!(position.dividends, FactAvailability::Unavailable);
    assert_eq!(
        position.open_position_gain_loss,
        FactAvailability::Unavailable
    );
    assert!(performance(&inventory)
        .aggregates
        .current_value
        .value()
        .is_some());
    assert_eq!(
        performance(&inventory).aggregates.invested_cost,
        FactAvailability::Unavailable
    );
    assert_eq!(
        performance(&inventory).aggregates.dividends,
        FactAvailability::Unavailable
    );
    assert_eq!(
        performance(&inventory).aggregates.open_position_gain_loss,
        FactAvailability::Unavailable
    );
    assert!(inventory
        .monetary_holdings
        .aggregates
        .invested_cost
        .value()
        .is_some());
    assert!(position
        .market_data_limitations
        .iter()
        .any(|limitation| matches!(
            limitation.subject,
            MarketDataSubject::FxRate { ref currency } if currency == "USD"
        )));
}

#[tokio::test]
async fn inventory_uses_latest_fx_on_or_before_each_transaction_date() {
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

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let position = &performance(&inventory).positions[0];

    // Buy costs are 21 * 0.8 and 22 * 0.9; the dividend is (10 - 1) * 0.9.
    assert!((position.invested_cost.value().copied().unwrap() - 36.6).abs() < 1e-9);
    assert!((position.average_cost.value().copied().unwrap() - 12.2).abs() < 1e-9);
    assert!((position.dividends.value().copied().unwrap() - 8.1).abs() < 1e-9);
    assert!((position.current_value.value().copied().unwrap() - 45.0).abs() < 1e-9);
    assert!((position.open_position_gain_loss.value().copied().unwrap() - 8.4).abs() < 1e-9);
    let aggregates = &performance(&inventory).aggregates;
    assert!((aggregates.invested_cost.value().copied().unwrap() - 36.6).abs() < 1e-9);
    assert!((aggregates.dividends.value().copied().unwrap() - 8.1).abs() < 1e-9);
    assert!((aggregates.open_position_gain_loss.value().copied().unwrap() - 8.4).abs() < 1e-9);
}

#[tokio::test]
async fn inventory_is_identical_across_requests_when_fx_is_source_available_but_uncached() {
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

    let first = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let second = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert_eq!(performance(&first).positions.len(), 1);
    let first_position = &performance(&first).positions[0];
    let second_position = &performance(&second).positions[0];
    // Cost and dividend facts must be complete on the first request already.
    assert!((first_position.invested_cost.value().copied().unwrap() - 16.0).abs() < 1e-9);
    assert!((first_position.average_cost.value().copied().unwrap() - 8.0).abs() < 1e-9);
    assert!((first_position.dividends.value().copied().unwrap() - 2.4).abs() < 1e-9);
    assert_eq!(first_position.quantity, second_position.quantity);
    assert_eq!(first_position.invested_cost, second_position.invested_cost);
    assert_eq!(first_position.average_cost, second_position.average_cost);
    assert_eq!(first_position.dividends, second_position.dividends);
    assert_eq!(first_position.current_value, second_position.current_value);
    assert!(first_position.market_data_limitations.is_empty());
    assert!(
        (performance(&first)
            .aggregates
            .invested_cost
            .value()
            .copied()
            .unwrap()
            - 16.0)
            .abs()
            < 1e-9
    );
    assert!(
        (performance(&first)
            .aggregates
            .dividends
            .value()
            .copied()
            .unwrap()
            - 2.4)
            .abs()
            < 1e-9
    );
    assert!(
        (performance(&second)
            .aggregates
            .invested_cost
            .value()
            .copied()
            .unwrap()
            - 16.0)
            .abs()
            < 1e-9
    );
    assert!(
        (performance(&second)
            .aggregates
            .dividends
            .value()
            .copied()
            .unwrap()
            - 2.4)
            .abs()
            < 1e-9
    );
    assert_eq!(first.total_value, second.total_value);
}

#[tokio::test]
async fn inventory_keeps_independent_facts_when_buy_fx_is_missing() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEFX3", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-02", 3.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 12.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-02", 0.8).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-09", 0.9).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let position = &performance(&inventory).positions[0];

    assert_eq!(position.quantity, 2.0);
    assert_eq!(position.invested_cost, FactAvailability::Unavailable);
    assert_eq!(position.average_cost, FactAvailability::Unavailable);
    assert_eq!(position.current_value.value().copied(), Some(21.6));
    assert_eq!(position.dividends.value().copied(), Some(2.4));
    assert_eq!(
        position.open_position_gain_loss,
        FactAvailability::Unavailable
    );
    assert_eq!(
        position.open_position_gain_loss_pct,
        FactAvailability::Unavailable
    );
    assert_eq!(
        performance(&inventory)
            .aggregates
            .current_value
            .value()
            .copied(),
        Some(21.6)
    );
    assert_eq!(
        performance(&inventory).aggregates.invested_cost,
        FactAvailability::Unavailable
    );
    assert_eq!(
        performance(&inventory)
            .aggregates
            .dividends
            .value()
            .copied(),
        Some(2.4)
    );
    assert_eq!(
        performance(&inventory).aggregates.open_position_gain_loss,
        FactAvailability::Unavailable
    );
    assert_eq!(
        performance(&inventory)
            .aggregates
            .open_position_gain_loss_pct,
        FactAvailability::Unavailable
    );
    assert_eq!(inventory.total_value.value().copied(), Some(21.6));
    assert!(position
        .market_data_limitations
        .iter()
        .any(|limitation| matches!(
            limitation.subject,
            MarketDataSubject::FxRate { ref currency } if currency == "USD"
        )));
}

#[tokio::test]
async fn inventory_keeps_quantity_and_current_value_when_dividend_fx_is_missing() {
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_asset(&db, "XFAKEFX4", "USD Dividend Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 0.0).await;
    common::insert_dividend_transaction(&db, asset_id, "2025-06-02", 3.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 12.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-06-09", 0.9).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let position = &performance(&inventory).positions[0];

    assert_eq!(position.quantity, 2.0);
    assert_eq!(position.invested_cost, FactAvailability::Unavailable);
    assert_eq!(position.dividends, FactAvailability::Unavailable);
    assert_eq!(position.current_value.value().copied(), Some(21.6));
    assert_eq!(
        position.open_position_gain_loss,
        FactAvailability::Unavailable
    );
    assert_eq!(
        performance(&inventory).aggregates.dividends,
        FactAvailability::Unavailable
    );
}

#[tokio::test]
async fn inventory_reopens_after_liquidation_with_fresh_cost_basis() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKEREOPEN", "Reopened Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 1.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-06-02", 2.0, 12.0, 3.0).await;
    common::insert_transaction(&db, asset_id, "2025-06-03", 1.0, 7.0, 0.5).await;
    common::insert_daily_price(&db, asset_id, "2025-06-09", 8.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let position = &performance(&inventory).positions[0];

    assert_eq!(position.quantity, 1.0);
    assert_eq!(position.invested_cost.value().copied(), Some(7.5));
    assert_eq!(position.average_cost.value().copied(), Some(7.5));
    assert_eq!(position.current_value.value().copied(), Some(8.0));
    assert_eq!(position.open_position_gain_loss.value().copied(), Some(0.5));
}

#[tokio::test]
async fn unpriced_holding_stays_visible_with_unavailable_valuation_aggregates() {
    let db = common::setup_test_db().await;
    let priced_id = common::insert_asset(&db, "XFAKEPRICE1", "Priced Stock", "stock", "EUR").await;
    let unpriced_id =
        common::insert_asset(&db, "XFAKEPRICE2", "Unpriced Stock", "stock", "EUR").await;
    common::insert_transaction(&db, priced_id, "2025-06-01", 2.0, 10.0, 0.0).await;
    common::insert_transaction(&db, unpriced_id, "2025-06-01", 1.0, 5.0, 0.0).await;
    common::insert_daily_price(&db, priced_id, "2025-06-09", 12.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let section = performance(&inventory);
    assert_eq!(section.positions.len(), 2);
    let unpriced = section
        .positions
        .iter()
        .find(|position| position.ticker == "XFAKEPRICE2")
        .unwrap();
    assert_eq!(unpriced.quantity, 1.0);
    assert_eq!(unpriced.invested_cost.value().copied(), Some(5.0));
    assert_eq!(unpriced.individual_price, FactAvailability::Unavailable);
    assert_eq!(unpriced.current_value, FactAvailability::Unavailable);
    assert_eq!(
        unpriced.open_position_gain_loss,
        FactAvailability::Unavailable
    );

    // The unpriced position makes the value aggregates unavailable while
    // cost and dividend aggregates stay complete across the section.
    assert_eq!(
        section.aggregates.current_value,
        FactAvailability::Unavailable
    );
    assert_eq!(
        section.aggregates.invested_cost.value().copied(),
        Some(25.0)
    );
    assert!(section.aggregates.dividends.value().is_some());
    let priced = section
        .positions
        .iter()
        .find(|position| position.ticker == "XFAKEPRICE1")
        .unwrap();
    assert_eq!(priced.current_value.value().copied(), Some(24.0));
    assert!(inventory.total_value.value().is_none());
    assert!(!section.market_data_limitations.is_empty());
}

#[tokio::test]
async fn monetary_only_inventory_populates_only_the_monetary_section() {
    let db = common::setup_test_db().await;
    let monetary_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKEONLY1",
        "Only Monetary Fund",
        "EUR",
        "F000ONLY",
    )
    .await;
    common::insert_transaction(&db, monetary_id, "2025-06-01", 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, monetary_id, "2025-06-09", 101.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert!(inventory.performance_holdings.positions.is_empty());
    assert_eq!(
        inventory.performance_holdings.aggregates.current_value,
        FactAvailability::Available(0.0)
    );
    assert!(inventory
        .performance_holdings
        .market_data_limitations
        .is_empty());
    let monetary_section = &inventory.monetary_holdings;
    assert_eq!(monetary_section.positions.len(), 1);
    assert_eq!(
        monetary_section.aggregates.current_value.value().copied(),
        Some(1010.0)
    );
    assert_eq!(inventory.total_value.value().copied(), Some(1010.0));
}

#[tokio::test]
async fn limitation_scopes_stay_separate_between_inventory_sections() {
    let db = common::setup_test_db().await;
    let stock_id = common::insert_asset(&db, "XFAKECLEAN", "Clean Stock", "stock", "EUR").await;
    let monetary_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKESTALEM",
        "Stale Monetary Fund",
        "EUR",
        "F000STALE",
    )
    .await;
    common::insert_transaction(&db, stock_id, "2025-06-02", 1.0, 10.0, 0.0).await;
    common::insert_daily_price(&db, stock_id, "2025-06-09", 11.0, false).await;
    common::insert_transaction(&db, monetary_id, "2025-06-01", 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, monetary_id, "2025-06-01", 100.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert!(performance(&inventory).market_data_limitations.is_empty());
    assert_eq!(inventory.monetary_holdings.market_data_limitations.len(), 1);
    assert!(matches!(
        inventory.monetary_holdings.market_data_limitations[0].subject,
        MarketDataSubject::Asset { .. }
    ));
    // The stale Monetary price limits only its section's current value.
    assert_eq!(
        inventory
            .monetary_holdings
            .aggregates
            .current_value
            .value()
            .copied(),
        Some(1000.0)
    );
    assert!(
        performance(&inventory)
            .aggregates
            .current_value
            .value()
            .copied()
            .unwrap()
            - 11.0
            < 1e-9
    );
}

#[tokio::test]
async fn composition_consumes_performance_inventory_section_without_nav_rebuild() {
    let db = common::setup_test_db().await;
    let stock_id =
        common::insert_asset(&db, "XFAKECOMP1", "Composition Stock", "stock", "EUR").await;
    let monetary_id = common::insert_monetary_fund_asset(
        &db,
        "XFAKECOMP2",
        "Composition Monetary Fund",
        "EUR",
        "F000COMP",
    )
    .await;
    common::insert_transaction(&db, stock_id, "2025-06-02", 1.0, 100.0, 0.0).await;
    common::insert_transaction(&db, monetary_id, "2025-06-02", 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, stock_id, "2025-06-09", 110.0, false).await;
    common::insert_daily_price(&db, monetary_id, "2025-06-09", 101.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = composition::compute_composition(&db, &market_data)
        .await
        .unwrap();

    let breakdown = result.asset_class_breakdown.as_ref().unwrap();
    assert_eq!(breakdown.len(), 1);
    // Weights use the performance section only, never the Monetary holding.
    assert!((breakdown[0].weight - 100.0).abs() < 1e-9);
    assert!(result.top_holdings.as_ref().unwrap()[0].ticker.as_deref() == Some("XFAKECOMP1"));
    assert!(result.market_data_limitations.is_empty());
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn liquidated_portfolio_returns_empty_sections_with_known_zero_aggregates() {
    let db = common::setup_test_db().await;
    let asset_id = common::insert_asset(&db, "XFAKELIQ1", "Liquidated Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, "2025-06-01", 2.0, 10.0, 0.0).await;
    common::insert_sell_transaction(&db, asset_id, "2025-06-02", 2.0, 12.0, 0.0).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert!(inventory.performance_holdings.positions.is_empty());
    assert!(inventory.monetary_holdings.positions.is_empty());
    assert_eq!(
        inventory.performance_holdings.aggregates.invested_cost,
        FactAvailability::Available(0.0)
    );
    assert_eq!(inventory.total_value, FactAvailability::Available(0.0));
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn inventory_suppresses_acceptable_morningstar_lag_limitation() {
    let db = common::setup_test_db().await;
    let price_date = "2025-06-03";
    let asset_id = common::insert_fund_asset(&db, "XFAKEF1", "Fake Fund", "EUR", "F000FAKE").await;
    common::insert_transaction(&db, asset_id, price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, price_date, 100.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    assert!(performance(&inventory).market_data_limitations.is_empty());
}

#[tokio::test]
async fn inventory_surfaces_excessive_morningstar_lag_limitation() {
    let db = common::setup_test_db().await;
    let price_date = "2025-06-01";
    let asset_id =
        common::insert_fund_asset(&db, "XFAKEF2", "Delayed Fund", "EUR", "F000DELAY").await;
    common::insert_transaction(&db, asset_id, price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, price_date, 100.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    let limitations = &performance(&inventory).market_data_limitations;
    assert_eq!(limitations.len(), 1);
    assert_eq!(
        limitations[0].subject,
        MarketDataSubject::Asset {
            ticker: "XFAKEF2".to_owned(),
            name: "Delayed Fund".to_owned(),
            asset_type: rstock::models::AssetType::Fund,
        }
    );
}

#[tokio::test]
async fn inventory_surfaces_stock_stale_data_limitation() {
    let db = common::setup_test_db().await;
    let price_date = "2025-06-01";
    let asset_id = common::insert_asset(&db, "XFAKES1", "Stale Stock", "stock", "EUR").await;
    common::insert_transaction(&db, asset_id, price_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, price_date, 100.0, false).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    let limitations = &performance(&inventory).market_data_limitations;
    assert_eq!(limitations.len(), 1);
    assert_eq!(
        limitations[0].subject,
        MarketDataSubject::Asset {
            ticker: "XFAKES1".to_owned(),
            name: "Stale Stock".to_owned(),
            asset_type: rstock::models::AssetType::Stock,
        }
    );
}

#[tokio::test]
async fn inventory_surfaces_fx_stale_data_limitation() {
    let db = common::setup_test_db().await;
    let stale_fx_date = "2025-06-01";
    let fresh_price_date = "2025-06-09";
    let asset_id = common::insert_asset(&db, "XFAKES2", "USD Stock", "stock", "USD").await;
    common::insert_transaction(&db, asset_id, stale_fx_date, 10.0, 100.0, 0.0).await;
    common::insert_daily_price(&db, asset_id, stale_fx_date, 100.0, false).await;
    common::insert_daily_price(&db, asset_id, fresh_price_date, 110.0, false).await;
    common::insert_exchange_rate(&db, "USD", "EUR", stale_fx_date, 0.90).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    let limitations = &performance(&inventory).market_data_limitations;
    assert_eq!(limitations.len(), 1);
    assert_eq!(
        limitations[0].subject,
        MarketDataSubject::FxRate {
            currency: "USD".to_owned(),
        }
    );
}

#[tokio::test]
async fn composition_with_unpriced_holding_reports_performance_section_limitations() {
    let db = common::setup_test_db().await;
    let stock_id = common::insert_asset(&db, "XFAKEUNVAL", "Unvalued Stock", "stock", "EUR").await;
    common::insert_transaction(&db, stock_id, "2025-06-02", 1.0, 100.0, 0.0).await;

    let market_data = common::market_data_at(&common::MockMarketDataSources::new(), fixed_today());
    let result = composition::compute_composition(&db, &market_data)
        .await
        .unwrap();

    assert!(result.asset_class_breakdown.is_none());
    assert!(result.top_holdings.is_none());
    assert_eq!(result.market_data_limitations.len(), 1);
    assert!(matches!(
        result.market_data_limitations[0].subject,
        MarketDataSubject::Asset { ref ticker, .. } if ticker == "XFAKEUNVAL"
    ));
    assert!(portfolio_history_repo::find_latest(&db)
        .await
        .unwrap()
        .is_none());
}
