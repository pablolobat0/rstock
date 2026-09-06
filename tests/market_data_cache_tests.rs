pub mod common;

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use rstock::db::entities::asset;
use rstock::db::repos::{daily_price_repo, exchange_rate_repo};
use rstock::models::Asset;
use rstock::services::market_data::{MarketData, MarketDataSources, SourceObservation};
use sea_orm::EntityTrait;

#[derive(Clone, Debug, Eq, PartialEq)]
enum HistoricalCall {
    Stock(String, NaiveDate, NaiveDate),
    Fx(String, String, NaiveDate, NaiveDate),
}

#[derive(Clone, Default)]
struct RecordingSources {
    prices: Arc<HashMap<String, Vec<SourceObservation>>>,
    rates: Arc<HashMap<String, Vec<SourceObservation>>>,
    failures: Arc<HashSet<String>>,
    calls: Arc<Mutex<Vec<HistoricalCall>>>,
}

impl RecordingSources {
    fn with_data(
        prices: HashMap<String, Vec<SourceObservation>>,
        rates: HashMap<String, Vec<SourceObservation>>,
    ) -> Self {
        Self {
            prices: Arc::new(prices),
            rates: Arc::new(rates),
            ..Self::default()
        }
    }

    fn with_failures(failures: impl IntoIterator<Item = String>) -> Self {
        Self {
            failures: Arc::new(failures.into_iter().collect()),
            ..Self::default()
        }
    }

    fn calls(&self) -> Vec<HistoricalCall> {
        self.calls.lock().unwrap().clone()
    }

    fn observations(
        values: Option<&Vec<SourceObservation>>,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Vec<SourceObservation> {
        values
            .into_iter()
            .flatten()
            .filter(|observation| observation.date >= start && observation.date <= end)
            .cloned()
            .collect()
    }
}

#[async_trait::async_trait]
impl MarketDataSources for RecordingSources {
    async fn stock_price_history(
        &self,
        ticker: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.calls
            .lock()
            .unwrap()
            .push(HistoricalCall::Stock(ticker.to_owned(), start, end));
        if self.failures.contains(&format!("stock:{ticker}")) {
            anyhow::bail!("configured stock failure for {ticker}");
        }
        Ok(Self::observations(self.prices.get(ticker), start, end))
    }

    async fn fund_price_history(
        &self,
        code: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.stock_price_history(code, start, end).await
    }

    async fn exchange_rate_history(
        &self,
        from: &str,
        to: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> anyhow::Result<Vec<SourceObservation>> {
        self.calls.lock().unwrap().push(HistoricalCall::Fx(
            from.to_owned(),
            to.to_owned(),
            start,
            end,
        ));
        let pair = format!("{from}{to}");
        if self.failures.contains(&format!("fx:{pair}")) {
            anyhow::bail!("configured FX failure for {pair}");
        }
        Ok(Self::observations(self.rates.get(&pair), start, end))
    }

    async fn stock_info(&self, ticker: &str) -> anyhow::Result<rstock::models::StockInfo> {
        anyhow::bail!("cache test does not request stock info for {ticker}")
    }

    async fn fund_data(&self, code: &str, _limit: u32) -> anyhow::Result<rstock::models::FundData> {
        anyhow::bail!("cache test does not request fund data for {code}")
    }

    async fn fund_quote_metadata(
        &self,
        code: &str,
    ) -> anyhow::Result<rstock::models::FundQuoteMetadata> {
        anyhow::bail!("cache test does not request fund metadata for {code}")
    }
}

fn date(day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(2025, 1, day).unwrap()
}

fn observations(values: &[(u32, f64)]) -> Vec<SourceObservation> {
    values
        .iter()
        .map(|(day, value)| SourceObservation {
            date: date(*day),
            value: *value,
        })
        .collect()
}

async fn make_asset(db: &sea_orm::DatabaseConnection, ticker: &str, currency: &str) -> Asset {
    let id = common::insert_asset(db, ticker, ticker, "stock", currency).await;
    Asset::from(
        asset::Entity::find_by_id(id)
            .one(db)
            .await
            .unwrap()
            .unwrap(),
    )
}

fn market_data(sources: RecordingSources) -> MarketData {
    MarketData::new_with_clock(Box::new(sources), &common::TestClock::new(date(6)))
}

#[tokio::test]
async fn fully_warm_asset_and_fx_coverage_makes_no_source_requests() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db, "XFAKEUSD", "USD").await;
    for day in 1..=5 {
        common::insert_daily_price(&db, asset.id, &format!("2025-01-{day:02}"), 100.0, false).await;
        common::insert_exchange_rate(&db, "USD", "EUR", &format!("2025-01-{day:02}"), 0.9).await;
    }
    let sources = RecordingSources::default();

    let prepared = market_data(sources.clone())
        .prepare_valuation_market_data(&db, &[asset], "2025-01-01", "2025-01-05")
        .await
        .unwrap();

    assert_eq!(prepared.effective_end, date(5));
    assert!(prepared.limitations.is_empty());
    assert!(sources.calls().is_empty());
}

#[tokio::test]
async fn partial_asset_and_fx_coverage_requests_only_missing_intervals() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db, "XFAKEUSD", "USD").await;
    for (day, price, rate) in [(1, 10.0, 0.8), (2, 20.0, 0.81), (5, 50.0, 0.85)] {
        common::insert_daily_price(&db, asset.id, &format!("2025-01-{day:02}"), price, false).await;
        common::insert_exchange_rate(&db, "USD", "EUR", &format!("2025-01-{day:02}"), rate).await;
    }
    let sources = RecordingSources::with_data(
        HashMap::from([("XFAKEUSD".to_owned(), observations(&[(3, 30.0), (4, 40.0)]))]),
        HashMap::from([("USDEUR".to_owned(), observations(&[(3, 0.82), (4, 0.83)]))]),
    );

    market_data(sources.clone())
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-05",
        )
        .await
        .unwrap();

    assert_eq!(
        sources.calls(),
        vec![
            HistoricalCall::Stock("XFAKEUSD".to_owned(), date(3), date(4)),
            HistoricalCall::Fx("USD".to_owned(), "EUR".to_owned(), date(3), date(4)),
        ]
    );
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-03")
            .await
            .unwrap(),
        Some(30.0)
    );
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-05")
            .await
            .unwrap(),
        Some(50.0)
    );
    assert_eq!(
        common::find_exchange_rate(&db, "USD", "EUR", "2025-01-04")
            .await
            .unwrap(),
        Some(0.83)
    );
    assert_eq!(
        common::find_exchange_rate(&db, "USD", "EUR", "2025-01-05")
            .await
            .unwrap(),
        Some(0.85)
    );
}

#[tokio::test]
async fn bounded_cache_gap_is_forward_filled_and_becomes_warm() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db, "XFAKE1", "EUR").await;
    common::insert_daily_price(&db, asset.id, "2025-01-01", 10.0, false).await;
    common::insert_daily_price(&db, asset.id, "2025-01-04", 14.0, false).await;
    let sources = RecordingSources::default();
    let market_data = market_data(sources.clone());

    market_data
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-04",
        )
        .await
        .unwrap();
    market_data
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-04",
        )
        .await
        .unwrap();

    assert_eq!(
        sources.calls(),
        vec![HistoricalCall::Stock("XFAKE1".to_owned(), date(2), date(3))]
    );
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-03")
            .await
            .unwrap(),
        Some(10.0)
    );
}

#[tokio::test]
async fn completed_date_preparation_ignores_cached_same_day_rows() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db, "XFAKE1", "EUR").await;
    common::insert_daily_price(&db, asset.id, "2025-01-05", 10.0, false).await;
    common::insert_daily_price(&db, asset.id, "2025-01-06", 99.0, false).await;
    let sources = RecordingSources::with_data(
        HashMap::from([("XFAKE1".to_owned(), observations(&[(5, 10.0)]))]),
        HashMap::new(),
    );
    let prepared = market_data(sources.clone())
        .prepare_valuation_market_data(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-06",
        )
        .await
        .unwrap();

    assert_eq!(prepared.effective_end, date(5));
    assert_eq!(sources.calls().len(), 1);
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-06")
            .await
            .unwrap(),
        Some(99.0)
    );
}

#[tokio::test]
async fn immutable_bulk_writes_preserve_successes_and_replace_failure_markers() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db, "XFAKE1", "EUR").await;
    common::insert_daily_price(&db, asset.id, "2025-01-01", 10.0, false).await;
    common::insert_daily_price(&db, asset.id, "2025-01-02", 0.0, true).await;
    common::insert_exchange_rate(&db, "USD", "EUR", "2025-01-01", 0.8).await;

    daily_price_repo::insert_many_immutable(
        &db,
        &[
            daily_price_repo::DailyPriceWrite {
                asset_id: asset.id,
                date: "2025-01-01".to_owned(),
                price: 99.0,
                is_api_failure: false,
            },
            daily_price_repo::DailyPriceWrite {
                asset_id: asset.id,
                date: "2025-01-02".to_owned(),
                price: 20.0,
                is_api_failure: false,
            },
        ],
    )
    .await
    .unwrap();
    exchange_rate_repo::insert_many_immutable(
        &db,
        &[exchange_rate_repo::ExchangeRateWrite {
            from_currency: "USD".to_owned(),
            to_currency: "EUR".to_owned(),
            date: "2025-01-01".to_owned(),
            rate: 0.99,
        }],
    )
    .await
    .unwrap();

    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-01")
            .await
            .unwrap(),
        Some(10.0)
    );
    assert_eq!(
        common::find_daily_price(&db, asset.id, "2025-01-02")
            .await
            .unwrap(),
        Some(20.0)
    );
    assert_eq!(
        common::find_exchange_rate(&db, "USD", "EUR", "2025-01-01")
            .await
            .unwrap(),
        Some(0.8)
    );
}

#[tokio::test]
async fn identical_fx_requests_share_success_and_failure_within_one_command() {
    let sources = RecordingSources::with_failures(["fx:USDEUR".to_owned()]);
    let first_command = market_data(sources.clone());

    let (first, second) = tokio::join!(
        first_command.exchange_rate_history("USD", "EUR", date(1), date(5)),
        first_command.exchange_rate_history("USD", "EUR", date(1), date(5))
    );

    assert!(first.is_err());
    assert!(second.is_err());
    assert_eq!(sources.calls().len(), 1);

    let next_command = market_data(sources.clone());
    assert!(next_command
        .exchange_rate_history("USD", "EUR", date(1), date(5))
        .await
        .is_err());
    assert_eq!(sources.calls().len(), 2);
}

#[tokio::test]
async fn preparation_reuses_empty_success_for_the_command_and_retries_next_command() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db, "XFAKE1", "EUR").await;
    let sources = RecordingSources::default();
    let first_command = market_data(sources.clone());

    first_command
        .prepare_valuation_market_data_if_available(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-02",
        )
        .await
        .unwrap();
    first_command
        .prepare_valuation_market_data_if_available(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-02",
        )
        .await
        .unwrap();

    assert_eq!(sources.calls().len(), 1);

    market_data(sources.clone())
        .prepare_valuation_market_data_if_available(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-02",
        )
        .await
        .unwrap();
    assert_eq!(sources.calls().len(), 2);
}

#[tokio::test]
async fn preparation_reuses_failed_result_for_the_command_and_retries_next_command() {
    let db = common::setup_test_db().await;
    let asset = make_asset(&db, "XFAKE1", "EUR").await;
    let sources = RecordingSources::with_failures(["stock:XFAKE1".to_owned()]);
    let first_command = market_data(sources.clone());

    for _ in 0..2 {
        let prepared = first_command
            .prepare_valuation_market_data_if_available(
                &db,
                std::slice::from_ref(&asset),
                "2025-01-01",
                "2025-01-02",
            )
            .await
            .unwrap();
        assert!(!prepared.data_available);
    }
    assert_eq!(sources.calls().len(), 1);

    let prepared = market_data(sources.clone())
        .prepare_valuation_market_data_if_available(
            &db,
            std::slice::from_ref(&asset),
            "2025-01-01",
            "2025-01-02",
        )
        .await
        .unwrap();
    assert!(!prepared.data_available);
    assert_eq!(sources.calls().len(), 2);
}

#[tokio::test]
async fn preparation_preserves_independent_success_when_an_asset_request_fails() {
    let db = common::setup_test_db().await;
    let successful = make_asset(&db, "XFAKE1", "EUR").await;
    let failed = make_asset(&db, "XFAKE2", "EUR").await;
    let mut sources = RecordingSources::with_data(
        HashMap::from([("XFAKE1".to_owned(), observations(&[(1, 10.0), (2, 11.0)]))]),
        HashMap::new(),
    );
    sources.failures = Arc::new(HashSet::from(["stock:XFAKE2".to_owned()]));

    let prepared = market_data(sources.clone())
        .prepare_valuation_market_data_if_available(
            &db,
            &[successful.clone(), failed],
            "2025-01-01",
            "2025-01-02",
        )
        .await
        .unwrap();

    assert!(!prepared.data_available);
    assert_eq!(prepared.limitations.len(), 1);
    assert_eq!(sources.calls().len(), 2);
    assert_eq!(
        common::find_daily_price(&db, successful.id, "2025-01-02")
            .await
            .unwrap(),
        Some(11.0)
    );
}
