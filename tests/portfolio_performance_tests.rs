mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use chrono::{Duration as ChronoDuration, NaiveDate};
use rstock::models::{FundData, FundQuoteMetadata, StockInfo};
use rstock::services::market_data::{MarketData, MarketDataSources, SourceObservation};
use rstock::services::portfolio;

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
    let result = portfolio::get_portfolio(&db, &market_data).await.unwrap();
    let elapsed = started.elapsed();

    assert_eq!(result.rows.len(), 5);
    assert!(result
        .rows
        .iter()
        .all(|position| position.current_price == Some(100.0)
            && position.price_date.as_deref() == Some("2025-06-09")));
    assert_eq!(counters.calls.load(Ordering::Relaxed), 10);
    assert_eq!(counters.active.load(Ordering::Relaxed), 0);
    assert_eq!(counters.peak.load(Ordering::Relaxed), 4);
    assert!(
        elapsed < Duration::from_millis(180),
        "bounded quote work should overlap independent requests: {elapsed:?}"
    );
}
