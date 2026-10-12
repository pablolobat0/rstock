pub mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::Result;
use chrono::NaiveDate;
use common::{insert_asset, setup_test_db};
use rstock::db::repos::asset_repo;
use rstock::models::{FundData, FundQuoteMetadata, MarketDataSubject, StockInfo};
use rstock::services::market_data::{MarketData, MarketDataSources, SourceObservation};
use tokio::time::{sleep, Duration};

#[derive(Clone)]
struct DelayedSources {
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    calls: Arc<AtomicUsize>,
    failing_ticker: Option<String>,
}

impl DelayedSources {
    fn new(failing_ticker: Option<&str>) -> Self {
        Self {
            active: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            calls: Arc::new(AtomicUsize::new(0)),
            failing_ticker: failing_ticker.map(str::to_owned),
        }
    }

    async fn delayed_call(&self) {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let active = self.active.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak.fetch_max(active, Ordering::Relaxed);
        sleep(Duration::from_millis(10)).await;
        self.active.fetch_sub(1, Ordering::Relaxed);
    }
}

#[async_trait::async_trait]
impl MarketDataSources for DelayedSources {
    async fn stock_price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        _: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        self.delayed_call().await;
        if self.failing_ticker.as_deref() == Some(ticker) {
            anyhow::bail!("offline source failure for {ticker}");
        }
        Ok(vec![SourceObservation {
            date: start,
            value: 100.0,
        }])
    }

    async fn fund_price_history(
        &self,
        code: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        self.stock_price_history(code, start, end).await
    }

    async fn exchange_rate_history(
        &self,
        _: &str,
        _: &str,
        start: NaiveDate,
        _: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        self.delayed_call().await;
        Ok(vec![SourceObservation {
            date: start,
            value: 1.0,
        }])
    }

    async fn stock_info(&self, ticker: &str) -> Result<StockInfo> {
        anyhow::bail!("offline test does not request stock info ({ticker})")
    }

    async fn fund_data(&self, code: &str, _: u32) -> Result<FundData> {
        anyhow::bail!("offline test does not request fund data ({code})")
    }

    async fn fund_quote_metadata(&self, code: &str) -> Result<FundQuoteMetadata> {
        anyhow::bail!("offline test does not request fund metadata ({code})")
    }
}

async fn load_assets(
    db: &sea_orm::DatabaseConnection,
    tickers: &[&str],
) -> Vec<rstock::models::Asset> {
    let ids = futures::future::join_all(
        tickers
            .iter()
            .map(|ticker| insert_asset(db, ticker, ticker, "stock", "EUR")),
    )
    .await;
    asset_repo::find_by_ids(db, ids).await.unwrap()
}

#[tokio::test]
async fn historical_source_requests_are_bounded_and_failures_are_isolated() {
    let db = setup_test_db().await;
    let tickers = [
        "XCONC01", "XCONC02", "XCONC03", "XCONC04", "XCONC05", "XCONC06", "XCONC07", "XFAIL01",
    ];
    let assets = load_assets(&db, &tickers).await;
    let sources = DelayedSources::new(Some("XFAIL01"));
    let market_data = MarketData::new_with_clock(
        Box::new(sources.clone()),
        &common::TestClock::new(NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()),
    );
    let mut requested_assets = assets.clone();
    requested_assets.push(
        assets
            .iter()
            .find(|asset| asset.ticker == "XFAIL01")
            .unwrap()
            .clone(),
    );

    let preparation = market_data
        .prepare_valuation_market_data_if_available(
            &db,
            &requested_assets,
            "2025-01-01",
            "2025-01-01",
        )
        .await
        .unwrap();

    assert!(!preparation.data_available);
    assert_eq!(sources.calls.load(Ordering::Relaxed), tickers.len());
    assert_eq!(sources.peak.load(Ordering::Relaxed), 4);
    assert_eq!(sources.active.load(Ordering::Relaxed), 0);
    assert!(preparation.limitations.iter().any(|limitation| {
        matches!(
            &limitation.subject,
            MarketDataSubject::Asset { ticker, .. } if ticker == "XFAIL01"
        )
    }));
    for asset in assets.iter().filter(|asset| asset.ticker != "XFAIL01") {
        assert_eq!(
            common::find_daily_price(&db, asset.id, "2025-01-01")
                .await
                .unwrap(),
            Some(100.0)
        );
    }
}

#[tokio::test]
async fn identical_source_requests_share_one_attempt() {
    let db = setup_test_db().await;
    let assets = load_assets(&db, &["XDEDUP1"]).await;
    let sources = DelayedSources::new(None);
    let market_data = MarketData::new_with_clock(
        Box::new(sources.clone()),
        &common::TestClock::new(NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()),
    );

    market_data
        .prepare_valuation_market_data(
            &db,
            &[assets[0].clone(), assets[0].clone()],
            "2025-01-01",
            "2025-01-01",
        )
        .await
        .unwrap();

    assert_eq!(sources.calls.load(Ordering::Relaxed), 1);
}

#[tokio::test]
async fn nav_preparation_overlaps_independent_asset_and_fx_requests() {
    let db = setup_test_db().await;
    for (ticker, currency) in [("XFAKENAVUSD", "USD"), ("XFAKENAVGBP", "GBP")] {
        let id = insert_asset(&db, ticker, ticker, "stock", currency).await;
        common::insert_transaction(&db, id, "2025-01-02", 1.0, 100.0, 0.0).await;
    }
    let sources = DelayedSources::new(None);
    let market_data = MarketData::new_with_clock(
        Box::new(sources.clone()),
        &common::TestClock::new(NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()),
    );

    let readiness = rstock::services::nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    let snapshot = readiness.latest_snapshot.unwrap();
    assert_eq!(snapshot.date, "2025-01-02");
    assert_eq!(snapshot.total_value, 200.0);
    assert_eq!(snapshot.nav, 100.0);
    assert_eq!(readiness.execution_database_reads, 0);
    assert_eq!(sources.calls.load(Ordering::Relaxed), 4);
    assert_eq!(
        sources.peak.load(Ordering::Relaxed),
        4,
        "NAV must overlap both asset and FX preparation rather than run sequential phases"
    );
    assert_eq!(sources.active.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn concurrent_nav_preparation_preserves_success_when_an_asset_fails() {
    let db = setup_test_db().await;
    let tickers = [
        "XFAKENAV01",
        "XFAKENAV02",
        "XFAKENAV03",
        "XFAKENAV04",
        "XFAKENAV05",
        "XFAKENAVFAIL",
    ];
    let assets = load_assets(&db, &tickers).await;
    for asset in &assets {
        common::insert_transaction(&db, asset.id, "2025-01-02", 1.0, 100.0, 0.0).await;
    }
    let sources = DelayedSources::new(Some("XFAKENAVFAIL"));
    let market_data = MarketData::new_with_clock(
        Box::new(sources.clone()),
        &common::TestClock::new(NaiveDate::from_ymd_opt(2025, 1, 3).unwrap()),
    );

    let readiness = rstock::services::nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    assert!(readiness.latest_snapshot.is_none());
    assert!(readiness
        .market_data_limitations
        .iter()
        .any(|limitation| matches!(
            &limitation.subject, MarketDataSubject::Asset { ticker, .. } if ticker == "XFAKENAVFAIL"
        )));
    assert_eq!(sources.peak.load(Ordering::Relaxed), 4);
    assert_eq!(sources.active.load(Ordering::Relaxed), 0);
    for asset in assets.iter().filter(|asset| asset.ticker != "XFAKENAVFAIL") {
        assert_eq!(
            common::find_daily_price(&db, asset.id, "2025-01-02")
                .await
                .unwrap(),
            Some(100.0)
        );
    }
}

#[tokio::test]
async fn nav_preparation_keeps_one_assets_disjoint_intervals_sequential() {
    let db = setup_test_db().await;
    let id = insert_asset(&db, "XFAKENAVREOPEN", "Reopened stock", "stock", "EUR").await;
    common::insert_transaction(&db, id, "2025-01-02", 1.0, 100.0, 0.0).await;
    common::insert_sell_transaction(&db, id, "2025-01-03", 1.0, 100.0, 0.0).await;
    common::insert_transaction(&db, id, "2025-01-05", 1.0, 100.0, 0.0).await;
    let sources = DelayedSources::new(None);
    let market_data = MarketData::new_with_clock(
        Box::new(sources.clone()),
        &common::TestClock::new(NaiveDate::from_ymd_opt(2025, 1, 6).unwrap()),
    );

    let readiness = rstock::services::nav::ensure_portfolio_history(&db, &market_data)
        .await
        .unwrap();

    assert_eq!(readiness.latest_snapshot.unwrap().date, "2025-01-05");
    assert_eq!(sources.calls.load(Ordering::Relaxed), 2);
    assert_eq!(
        sources.peak.load(Ordering::Relaxed),
        1,
        "intervals for one asset share cache/predecessor state and must retain their ordering"
    );
    assert_eq!(sources.active.load(Ordering::Relaxed), 0);
}
