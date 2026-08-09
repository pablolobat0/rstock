//! Offline performance baseline.
//!
//! Fixture construction is deliberately outside every timed closure.  The
//! benchmark is also useful as a smoke test: it never constructs a production
//! source and therefore cannot make a network request.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::Result;
use chrono::{Duration, NaiveDate};
use criterion::{criterion_group, criterion_main, Criterion};
use migration::{Migrator, MigratorTrait};
use rstock::db::entities::{asset, transaction};
use rstock::db::repos::transaction_repo;
use rstock::models::{Asset, AssetType};
use rstock::services::market_data::{MarketData, MarketDataSources, SourceObservation};
use rstock::services::{analytics, nav, portfolio};
use sea_orm::{ActiveModelTrait, Database, DatabaseConnection, Set};
use tempfile::TempDir;

const START: &str = "2015-01-01";
const END: &str = "2015-12-31";

const FIXTURE_MATRIX: &[(usize, usize, usize)] = &[(5, 1, 100), (50, 10, 5_000), (100, 20, 20_000)];

#[derive(Default)]
struct Counters {
    source_calls: AtomicUsize,
    active: AtomicUsize,
    peak: AtomicUsize,
    requested_intervals: Mutex<Vec<(NaiveDate, NaiveDate)>>,
}

impl Counters {
    fn call(&self, start: NaiveDate, end: NaiveDate) {
        self.source_calls.fetch_add(1, Ordering::Relaxed);
        self.requested_intervals.lock().unwrap().push((start, end));
        let active = self.active.fetch_add(1, Ordering::Relaxed) + 1;
        self.peak.fetch_max(active, Ordering::Relaxed);
    }

    fn finish(&self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }
}

#[derive(Clone)]
struct OfflineSources {
    counters: Arc<Counters>,
    observations: Arc<Vec<SourceObservation>>,
    delay_ms: u64,
}

impl OfflineSources {
    fn observations(&self, start: NaiveDate, end: NaiveDate) -> Vec<SourceObservation> {
        self.observations
            .iter()
            .filter(|observation| observation.date >= start && observation.date <= end)
            .cloned()
            .collect()
    }
}

#[async_trait::async_trait]
impl MarketDataSources for OfflineSources {
    async fn stock_price_history(
        &self,
        _: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        self.counters.call(start, end);
        if self.delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(self.delay_ms)).await;
        }
        let result = self.observations(start, end);
        self.counters.finish();
        Ok(result)
    }

    async fn fund_price_history(
        &self,
        _: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        self.stock_price_history("fund", start, end).await
    }

    async fn exchange_rate_history(
        &self,
        _: &str,
        _: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Vec<SourceObservation>> {
        self.stock_price_history("fx", start, end).await
    }

    async fn stock_info(&self, ticker: &str) -> Result<rstock::models::StockInfo> {
        anyhow::bail!("offline benchmark does not request stock info ({ticker})")
    }

    async fn fund_data(&self, code: &str, _: u32) -> Result<rstock::models::FundData> {
        anyhow::bail!("offline benchmark does not request fund data ({code})")
    }

    async fn fund_quote_metadata(&self, code: &str) -> Result<rstock::models::FundQuoteMetadata> {
        anyhow::bail!("offline benchmark does not request fund metadata ({code})")
    }
}

struct Fixture {
    _tempdir: TempDir,
    db: DatabaseConnection,
    market_data: MarketData,
    assets: Vec<Asset>,
    counters: Arc<Counters>,
}

async fn build_fixture(asset_count: usize, years: usize, transaction_count: usize) -> Fixture {
    build_fixture_with_delay(asset_count, years, transaction_count, 0).await
}

async fn build_fixture_with_delay(
    asset_count: usize,
    years: usize,
    transaction_count: usize,
    delay_ms: u64,
) -> Fixture {
    let tempdir = tempfile::tempdir().expect("temporary benchmark directory");
    let path = tempdir.path().join("fixture.db");
    let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
        .await
        .expect("file-backed benchmark database");
    Migrator::up(&db, None).await.expect("benchmark migrations");

    let mut assets = Vec::with_capacity(asset_count);
    for index in 0..asset_count {
        let ticker = format!("XPERF{index:03}");
        let name = format!("Synthetic asset {index}");
        let asset_type = if index % 3 == 0 {
            AssetType::Fund
        } else {
            AssetType::Stock
        };
        let currency = if index % 2 == 0 { "EUR" } else { "USD" };
        let morningstar_code = (index % 3 == 0).then(|| format!("M{index:03}"));
        let id = asset::ActiveModel {
            ticker: Set(ticker),
            name: Set(name),
            asset_type: Set(asset_type.to_string()),
            currency: Set(currency.to_owned()),
            morningstar_code: Set(morningstar_code),
            created_at: Set("2015-01-01T00:00:00".into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("benchmark asset");
        assets.push(Asset::from(id));
    }

    for index in 0..transaction_count {
        let asset_id = assets[index % assets.len()].id;
        transaction::ActiveModel {
            asset_id: Set(asset_id),
            tx_type: Set(if index % 17 == 0 {
                "dividend".into()
            } else if index % 19 == 0 {
                "sell".into()
            } else if index % 23 == 0 {
                "split".into()
            } else {
                "buy".into()
            }),
            date: Set(format!(
                "{}-{:02}-{:02}",
                2015 + index % years,
                (index % 12) + 1,
                (index % 27) + 1
            )),
            quantity: Set(1.0),
            price_cents: Set(10_000 + i64::try_from(index % 100).unwrap()),
            fees_cents: Set(0),
            created_at: Set("2015-01-01T00:00:00".into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("benchmark transaction");
    }

    let start = NaiveDate::parse_from_str(START, "%Y-%m-%d").unwrap();
    let observations = (0..=i64::try_from(365 * years).unwrap())
        .filter(|offset| offset % 11 != 0)
        .map(|offset| SourceObservation {
            date: start + Duration::days(offset),
            value: 100.0 + offset as f64 / 10.0,
        })
        .collect();
    let counters = Arc::new(Counters::default());
    let market_data = MarketData::new_with_clock(
        Box::new(OfflineSources {
            counters: counters.clone(),
            observations: Arc::new(observations),
            delay_ms,
        }),
        &rstock::services::clock::FixedClock::new(NaiveDate::from_ymd_opt(2016, 1, 1).unwrap()),
    );
    Fixture {
        _tempdir: tempdir,
        db,
        market_data,
        assets,
        counters,
    }
}

#[allow(clippy::too_many_lines)]
fn benchmark_performance(c: &mut Criterion) {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let fixture = runtime.block_on(build_fixture(5, 1, 100));
    let representative = runtime.block_on(build_fixture(50, 10, 5_000));
    let stress = runtime.block_on(build_fixture(100, 20, 20_000));
    assert_eq!(FIXTURE_MATRIX.len(), 3);
    let mut group = c.benchmark_group("performance-baseline");
    group.bench_function("transaction_listing", |b| {
        b.to_async(&runtime).iter(|| async {
            transaction_repo::find_all_ordered_by_date(&fixture.db, None, Some(END))
                .await
                .unwrap()
        });
    });
    group.bench_function("transaction_listing_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            transaction_repo::find_all_ordered_by_date(&representative.db, None, None)
                .await
                .unwrap()
        });
    });
    group.bench_function("transaction_listing_stress", |b| {
        b.to_async(&runtime).iter(|| async {
            transaction_repo::find_all_ordered_by_date(&stress.db, None, None)
                .await
                .unwrap()
        });
    });
    group.bench_function("market_data_preparation_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            representative
                .market_data
                .prepare_valuation_market_data(
                    &representative.db,
                    &representative.assets,
                    START,
                    END,
                )
                .await
                .unwrap()
        });
    });
    group.bench_function("nav_rebuild_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            nav::ensure_portfolio_history(&representative.db, &representative.market_data)
                .await
                .unwrap()
        });
    });
    group.bench_function("portfolio_retrieval_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            let _ = portfolio::get_portfolio(&representative.db, &representative.market_data).await;
        });
    });
    group.bench_function("correlation_matrix_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            analytics::compute_correlation_data(
                &representative.db,
                START,
                END,
                &representative.market_data,
            )
            .await
            .unwrap()
        });
    });
    group.bench_function("rolling_correlation_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            analytics::compute_rolling_correlation_data(
                &representative.db,
                START,
                END,
                "XPERF001",
                "XPERF002",
                "1y",
                &representative.market_data,
            )
            .await
            .unwrap()
        });
    });
    group.bench_function("historical_market_data_preparation_cold", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = std::time::Duration::ZERO;
            for _ in 0..iterations {
                let fresh = runtime.block_on(build_fixture(5, 1, 100));
                let started = Instant::now();
                runtime
                    .block_on(fresh.market_data.prepare_valuation_market_data(
                        &fresh.db,
                        &fresh.assets,
                        START,
                        END,
                    ))
                    .unwrap();
                elapsed += started.elapsed();
            }
            elapsed
        });
    });
    group.bench_function("historical_market_data_preparation_warm", |b| {
        runtime
            .block_on(fixture.market_data.prepare_valuation_market_data(
                &fixture.db,
                &fixture.assets,
                START,
                END,
            ))
            .unwrap();
        b.to_async(&runtime).iter(|| async {
            fixture
                .market_data
                .prepare_valuation_market_data(&fixture.db, &fixture.assets, START, END)
                .await
                .unwrap()
        });
    });
    group.bench_function("historical_market_data_preparation_partial", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = std::time::Duration::ZERO;
            for _ in 0..iterations {
                let partial = runtime.block_on(build_fixture(5, 1, 100));
                runtime
                    .block_on(partial.market_data.prepare_valuation_market_data(
                        &partial.db,
                        &partial.assets[..1],
                        START,
                        END,
                    ))
                    .unwrap();
                let started = Instant::now();
                runtime
                    .block_on(partial.market_data.prepare_valuation_market_data(
                        &partial.db,
                        &partial.assets,
                        START,
                        END,
                    ))
                    .unwrap();
                elapsed += started.elapsed();
            }
            elapsed
        });
    });
    group.bench_function("nav_rebuild_full", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = std::time::Duration::ZERO;
            for _ in 0..iterations {
                let fresh = runtime.block_on(build_fixture(5, 1, 100));
                let started = Instant::now();
                runtime
                    .block_on(nav::ensure_portfolio_history(&fresh.db, &fresh.market_data))
                    .unwrap();
                elapsed += started.elapsed();
            }
            elapsed
        });
    });
    group.bench_function("portfolio_retrieval_cold", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = std::time::Duration::ZERO;
            for _ in 0..iterations {
                let fresh = runtime.block_on(build_fixture(5, 1, 100));
                let started = Instant::now();
                runtime
                    .block_on(portfolio::get_portfolio(&fresh.db, &fresh.market_data))
                    .ok();
                elapsed += started.elapsed();
            }
            elapsed
        });
    });
    let _ = runtime.block_on(portfolio::get_portfolio(&fixture.db, &fixture.market_data));
    group.bench_function("portfolio_retrieval_warm", |b| {
        b.to_async(&runtime).iter(|| async {
            let _ = portfolio::get_portfolio(&fixture.db, &fixture.market_data).await;
        });
    });
    group.bench_function("nav_rebuild_incremental", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = std::time::Duration::ZERO;
            for _ in 0..iterations {
                let incremental = runtime.block_on(build_fixture(5, 1, 100));
                runtime
                    .block_on(nav::ensure_portfolio_history(
                        &incremental.db,
                        &incremental.market_data,
                    ))
                    .unwrap();
                runtime
                    .block_on(rstock::db::repos::portfolio_history_repo::delete_from_date(
                        &incremental.db,
                        "2015-06-01",
                    ))
                    .unwrap();
                let started = Instant::now();
                runtime
                    .block_on(nav::ensure_portfolio_history(
                        &incremental.db,
                        &incremental.market_data,
                    ))
                    .unwrap();
                elapsed += started.elapsed();
            }
            elapsed
        });
    });
    group.bench_function("correlation_matrix", |b| {
        b.to_async(&runtime).iter(|| async {
            analytics::compute_correlation_data(&fixture.db, START, END, &fixture.market_data)
                .await
                .unwrap()
        });
    });
    group.bench_function("rolling_correlation", |b| {
        b.to_async(&runtime).iter(|| async {
            analytics::compute_rolling_correlation_data(
                &fixture.db,
                START,
                END,
                "XPERF001",
                "XPERF002",
                "1y",
                &fixture.market_data,
            )
            .await
            .unwrap()
        });
    });
    group.bench_function("startup_and_migration", |b| {
        b.iter_custom(|iterations| {
            let mut elapsed = std::time::Duration::ZERO;
            for _ in 0..iterations {
                let directory = tempfile::tempdir().unwrap();
                let path = directory.path().join("startup.db");
                let started = Instant::now();
                runtime.block_on(async {
                    let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
                        .await
                        .unwrap();
                    Migrator::up(&db, None).await.unwrap();
                });
                elapsed += started.elapsed();
            }
            elapsed
        });
    });
    for limit in [1_usize, 2, 4, 8] {
        let delayed = runtime.block_on(build_fixture_with_delay(5, 1, 100, 2));
        group.bench_function(format!("delayed_source_limit_{limit}"), |b| {
            b.to_async(&runtime).iter(|| async {
                // The candidate limit is part of the benchmark identity. The
                // current baseline source path is intentionally sequential;
                // later bounded-concurrency work replaces this seam.
                let _ = limit;
                delayed
                    .market_data
                    .prepare_valuation_market_data(&delayed.db, &delayed.assets, START, END)
                    .await
                    .unwrap()
            });
        });
    }
    group.finish();
    println!(
        "baseline source_calls={} peak={} intervals={:?}",
        fixture.counters.source_calls.load(Ordering::Relaxed),
        fixture.counters.peak.load(Ordering::Relaxed),
        fixture.counters.requested_intervals.lock().unwrap()
    );
    std::hint::black_box(fixture.counters.source_calls.load(Ordering::Relaxed));
    std::hint::black_box(fixture.counters.peak.load(Ordering::Relaxed));
}

criterion_group!(benches, benchmark_performance);
criterion_main!(benches);
