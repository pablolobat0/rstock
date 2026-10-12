pub mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{Duration as ChronoDuration, NaiveDate};
use rstock::models::{FundData, FundQuoteMetadata, StockInfo};
use rstock::services::market_data::{MarketData, MarketDataSources, SourceObservation};
use rstock::services::portfolio_inventory;

async fn staggered_fund_holdings() -> (sea_orm::DatabaseConnection, common::MockMarketDataSources) {
    let db = common::setup_test_db().await;
    let early =
        common::insert_fund_asset(&db, "XFAKEEARLY", "Early Fund", "EUR", "FAKEEARLY").await;
    let later =
        common::insert_fund_asset(&db, "XFAKELATER", "Later Fund", "EUR", "FAKELATER").await;
    common::insert_transaction(&db, early, "2025-06-02", 1.0, 100.0, 0.0).await;
    common::insert_transaction(&db, later, "2025-06-06", 1.0, 100.0, 0.0).await;
    for day in 2..=9 {
        let date = format!("2025-06-{day:02}");
        common::insert_daily_price(&db, early, &date, 100.0, false).await;
        if day >= 6 {
            common::insert_daily_price(&db, later, &date, 100.0, false).await;
        }
    }
    (db, common::MockMarketDataSources::new())
}

#[tokio::test]
async fn portfolio_view_reuses_nav_preparation_without_requesting_pre_holding_prices() {
    let (db, sources) = staggered_fund_holdings().await;
    let market_data =
        common::market_data_at(&sources, NaiveDate::from_ymd_opt(2025, 6, 10).unwrap());
    let view = rstock::services::portfolio_view::get_portfolio_view(
        &db,
        &market_data,
        rstock::models::NavHistoryRequest::All,
    )
    .await
    .unwrap();

    assert_eq!(view.inventory.performance_holdings.positions.len(), 2);
    let requests = sources.historical_price_request_ranges();
    assert!(
        requests.iter().all(|(code, _, _)| code != "FAKELATER"),
        "NAV already covers every held date; inventory must not request earlier fund prices: {requests:?}"
    );
}

#[tokio::test]
async fn standalone_inventory_does_not_request_prices_before_an_assets_first_transaction() {
    let (db, sources) = staggered_fund_holdings().await;
    let market_data =
        common::market_data_at(&sources, NaiveDate::from_ymd_opt(2025, 6, 10).unwrap());
    portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();

    let requests = sources.historical_price_request_ranges();
    assert!(
        requests.is_empty(),
        "each fund's own holding history is already cached: {requests:?}"
    );
}

#[tokio::test]
async fn portfolio_view_does_not_repeat_preparation_before_the_trusted_nav_checkpoint() {
    use rstock::db::entities::{daily_asset_price, portfolio_history};
    use rstock::db::repos::asset_repo;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

    let (db, sources) = staggered_fund_holdings().await;
    let early = asset_repo::find_by_ticker(&db, "XFAKEEARLY")
        .await
        .unwrap()
        .unwrap();
    let later = asset_repo::find_by_ticker(&db, "XFAKELATER")
        .await
        .unwrap()
        .unwrap();
    // The trusted snapshot needs only the following day's valuation. A hole
    // earlier in the cache must not trigger a second historical preparation.
    daily_asset_price::Entity::delete_many()
        .filter(daily_asset_price::Column::AssetId.eq(early.id))
        .filter(daily_asset_price::Column::Date.eq("2025-06-03"))
        .exec(&db)
        .await
        .unwrap();
    portfolio_history::ActiveModel {
        date: Set("2025-06-08".to_owned()),
        asset_value: Set(200.0),
        total_value: Set(200.0),
        outstanding_shares: Set(2.0),
        nav: Set(100.0),
    }
    .insert(&db)
    .await
    .unwrap();
    for asset_id in [early.id, later.id] {
        common::insert_portfolio_asset_snapshot(
            &db,
            "2025-06-08",
            asset_id,
            1.0,
            100.0,
            100.0,
            1.0,
        )
        .await;
    }
    let market_data =
        common::market_data_at(&sources, NaiveDate::from_ymd_opt(2025, 6, 10).unwrap());
    rstock::services::portfolio_view::get_portfolio_view(
        &db,
        &market_data,
        rstock::models::NavHistoryRequest::All,
    )
    .await
    .unwrap();
    let requests = sources.historical_price_request_ranges();
    assert!(
        requests
            .iter()
            .all(|(code, _, _)| code != "FAKEEARLY" && code != "FAKELATER"),
        "inventory must reuse NAV preparation: {requests:?}"
    );
}

#[tokio::test]
async fn portfolio_view_recovers_transaction_fx_before_the_nav_checkpoint() {
    use rstock::db::entities::portfolio_history;
    use sea_orm::{ActiveModelTrait, Set};
    let db = common::setup_test_db().await;
    let asset_id =
        common::insert_fund_asset(&db, "XFAKEOLDFX", "Old FX Fund", "USD", "FAKEOLDFX").await;
    common::insert_transaction(&db, asset_id, "2025-06-02", 1.0, 100.0, 0.0).await;
    for day in 2..=9 {
        common::insert_daily_price(&db, asset_id, &format!("2025-06-{day:02}"), 100.0, false).await;
    }
    for day in 8..=9 {
        common::insert_exchange_rate(&db, "USD", "EUR", &format!("2025-06-{day:02}"), 0.9).await;
    }
    portfolio_history::ActiveModel {
        date: Set("2025-06-08".into()),
        asset_value: Set(90.0),
        total_value: Set(90.0),
        outstanding_shares: Set(0.9),
        nav: Set(100.0),
    }
    .insert(&db)
    .await
    .unwrap();
    common::insert_portfolio_asset_snapshot(&db, "2025-06-08", asset_id, 1.0, 100.0, 90.0, 0.9)
        .await;
    let mut sources = common::MockMarketDataSources::new();
    sources.exchange_rates.insert(
        "USDEUR".into(),
        vec![("2025-06-02".into(), 0.8), ("2025-06-10".into(), 0.9)],
    );
    let market_data =
        common::market_data_at(&sources, NaiveDate::from_ymd_opt(2025, 6, 10).unwrap());
    let view = rstock::services::portfolio_view::get_portfolio_view(
        &db,
        &market_data,
        rstock::models::NavHistoryRequest::All,
    )
    .await
    .unwrap();
    assert_eq!(
        view.inventory.performance_holdings.positions[0].invested_cost,
        rstock::models::FactAvailability::Available(80.0)
    );
}

#[tokio::test]
async fn inventory_consolidates_overlapping_historical_fx_requests_by_currency() {
    let db = common::setup_test_db().await;
    for (ticker, code, first_day) in [
        ("XFAKEFXEARLY", "FAKEFXEARLY", 2),
        ("XFAKEFXLATER", "FAKEFXLATER", 6),
    ] {
        let asset_id = common::insert_fund_asset(&db, ticker, "USD Fund", "USD", code).await;
        common::insert_transaction(
            &db,
            asset_id,
            &format!("2025-06-{first_day:02}"),
            1.0,
            100.0,
            0.0,
        )
        .await;
        for day in first_day..=9 {
            common::insert_daily_price(&db, asset_id, &format!("2025-06-{day:02}"), 100.0, false)
                .await;
        }
    }
    let mut sources = common::MockMarketDataSources::new();
    sources.exchange_rate_delay = Duration::from_millis(20);
    sources.exchange_rates.insert(
        "USDEUR".into(),
        (2..=10)
            .map(|day| (format!("2025-06-{day:02}"), 0.9))
            .collect(),
    );
    let today = NaiveDate::from_ymd_opt(2025, 6, 10).unwrap();
    let market_data = common::market_data_at(&sources, today);
    let inventory = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    assert!(inventory.performance_holdings.positions.iter().all(
        |position| position.invested_cost == rstock::models::FactAvailability::Available(90.0)
    ));
    let requests = sources.historical_fx_requests.lock().unwrap();
    let historical: Vec<_> = requests.iter().filter(|(_, _, end)| *end < today).collect();
    assert_eq!(
        historical.len(),
        1,
        "one shared currency must not trigger overlapping source attempts: {historical:?}"
    );
}

#[derive(Default)]
struct Counters {
    calls: AtomicUsize,
    active: AtomicUsize,
    peak: AtomicUsize,
}

impl Counters {
    fn start(&self) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let active = self.active.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak.fetch_max(active, Ordering::Relaxed);
    }

    fn finish(&self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
struct DelayedSources {
    counters: Arc<Counters>,
    yesterday: NaiveDate,
}

#[async_trait::async_trait]
impl MarketDataSources for DelayedSources {
    async fn stock_price_history(
        &self,
        _ticker: &str,
        _start: NaiveDate,
        _end: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        self.counters.start();
        tokio::time::sleep(Duration::from_millis(20)).await;
        self.counters.finish();
        Ok(vec![SourceObservation {
            date: self.yesterday,
            value: 100.0,
        }])
    }

    async fn fund_price_history(
        &self,
        _code: &str,
        _start: NaiveDate,
        _end: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        Ok(Vec::new())
    }

    async fn exchange_rate_history(
        &self,
        _from: &str,
        _to: &str,
        _start: NaiveDate,
        _end: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        Ok(Vec::new())
    }

    async fn stock_info(&self, ticker: &str) -> Result<StockInfo> {
        anyhow::bail!("unexpected stock info request for {ticker}")
    }

    async fn fund_data(&self, code: &str, _limit: u32) -> Result<FundData> {
        anyhow::bail!("unexpected fund data request for {code}")
    }

    async fn fund_quote_metadata(&self, code: &str) -> Result<FundQuoteMetadata> {
        anyhow::bail!("unexpected fund quote metadata request for {code}")
    }
}

#[tokio::test]
async fn portfolio_quotes_are_bounded_and_use_historical_fallback_without_duplicate_work() {
    let db = common::setup_test_db().await;
    let today = NaiveDate::from_ymd_opt(2025, 6, 10).unwrap();
    let yesterday = today - ChronoDuration::days(1);
    for index in 0..5 {
        let asset_id = common::insert_asset(
            &db,
            &format!("XFAKEPERF{index}"),
            "Performance fixture",
            "stock",
            "EUR",
        )
        .await;
        common::insert_transaction(&db, asset_id, "2025-06-10", 1.0, 90.0, 0.0).await;
    }
    let counters = Arc::new(Counters::default());
    let market_data = MarketData::new_with_clock(
        Box::new(DelayedSources {
            counters: counters.clone(),
            yesterday,
        }),
        &common::TestClock::new(today),
    );

    let started = Instant::now();
    let result = portfolio_inventory::get_portfolio_inventory(&db, &market_data)
        .await
        .unwrap();
    let elapsed = started.elapsed();

    assert_eq!(result.performance_holdings.positions.len(), 5);
    assert!(result
        .performance_holdings
        .positions
        .iter()
        .all(|position| matches!(&position.individual_price, rstock::models::FactAvailability::Available(price)
            if (price.native_price - 100.0).abs() < 1e-9 && price.price_date == "2025-06-09")));
    assert_eq!(counters.calls.load(Ordering::Relaxed), 10);
    assert_eq!(counters.active.load(Ordering::Relaxed), 0);
    assert_eq!(counters.peak.load(Ordering::Relaxed), 4);
    assert!(
        elapsed < Duration::from_millis(180),
        "bounded quote work should overlap independent requests: {elapsed:?}"
    );
}
