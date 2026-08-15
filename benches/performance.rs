//! Offline performance baseline.
//!
//! Fixture construction is deliberately outside every timed closure.  The
//! benchmark uses an injected offline source for service paths. Executable
//! startup paths use low-work commands with unreachable source settings and
//! fail if those commands unexpectedly try to fetch market data.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use anyhow::Result;
use chrono::{Duration, NaiveDate};
use criterion::{criterion_group, criterion_main, Criterion};
use futures::stream::{self, StreamExt};
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

fn release_binary() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join("rstock")
}

fn release_logging_binary() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join("startup_logging")
}

fn run_rstock(binary: &Path, home: &Path, args: &[&str]) {
    let status = Command::new(binary)
        .args(args)
        .env("HOME", home)
        .env_remove("RUST_LOG")
        .env(
            "RSTOCK_SOURCE_TOKEN_CACHE_PATH",
            home.join("offline-token.json"),
        )
        .envs([
            ("RSTOCK_SOURCE_TOKEN_PAGE_URL", "file:///offline/token"),
            ("RSTOCK_SOURCE_CHARTSERVICE_URL", "file:///offline/chart"),
            ("RSTOCK_SOURCE_HOLDINGS_URL", "file:///offline/holdings"),
            ("RSTOCK_SOURCE_QUOTE_URL", "file:///offline/quote"),
            ("RSTOCK_SOURCE_SAL_API_KEY", "offline"),
            ("RSTOCK_SOURCE_USER_AGENT", "rstock-offline-benchmark"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("release rstock executable should run");
    assert!(status.success(), "rstock command should succeed");
}

fn run_logging_init(binary: &Path, home: &Path) {
    let status = Command::new(binary)
        .env("HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("release logging executable should run");
    assert!(status.success(), "logging initialization should succeed");
}

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
    end: String,
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
    build_fixture_with_counters(asset_count, years, transaction_count, delay_ms, None).await
}

async fn build_fixture_with_counters(
    asset_count: usize,
    years: usize,
    transaction_count: usize,
    delay_ms: u64,
    shared_counters: Option<Arc<Counters>>,
) -> Fixture {
    let start = NaiveDate::parse_from_str(START, "%Y-%m-%d").unwrap();
    let end = NaiveDate::from_ymd_opt(2015 + i32::try_from(years).unwrap() - 1, 12, 31).unwrap();
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
        let asset_type = if index % 3 == 2 {
            AssetType::Fund
        } else {
            AssetType::Stock
        };
        let currency = if index % 2 == 0 { "EUR" } else { "USD" };
        let morningstar_code = (index % 3 == 2).then(|| format!("M{index:03}"));
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
        let offset = i64::try_from(index).unwrap() * (end - start).num_days()
            / i64::try_from(transaction_count).unwrap();
        transaction::ActiveModel {
            asset_id: Set(asset_id),
            tx_type: Set(if index < asset_count {
                "buy".into()
            } else if index % 17 == 0 {
                "dividend".into()
            } else if index % 19 == 0 {
                "sell".into()
            } else if index % 23 == 0 {
                "split".into()
            } else {
                "buy".into()
            }),
            date: Set((start + Duration::days(offset))
                .format("%Y-%m-%d")
                .to_string()),
            quantity: Set(if index < asset_count { 100.0 } else { 1.0 }),
            price_cents: Set(10_000 + i64::try_from(index % 100).unwrap()),
            fees_cents: Set(0),
            created_at: Set("2015-01-01T00:00:00".into()),
            ..Default::default()
        }
        .insert(&db)
        .await
        .expect("benchmark transaction");
    }

    let observations = (0..=(end - start).num_days())
        .filter(|offset| delay_ms > 0 || *offset == 0 || offset % 11 != 0)
        .map(|offset| SourceObservation {
            date: start + Duration::days(offset),
            value: 100.0 + offset as f64 / 10.0,
        })
        .collect();
    let counters = shared_counters.unwrap_or_default();
    let market_data = MarketData::new_with_clock(
        Box::new(OfflineSources {
            counters: counters.clone(),
            observations: Arc::new(observations),
            delay_ms,
        }),
        &rstock::services::clock::FixedClock::new(end + Duration::days(1)),
    );
    Fixture {
        _tempdir: tempdir,
        db,
        market_data,
        assets,
        counters,
        end: end.format("%Y-%m-%d").to_string(),
    }
}

async fn run_delayed_candidate(limit: usize) -> (std::time::Duration, usize, usize) {
    let shared_counters = Arc::new(Counters::default());
    let mut fixtures = Vec::new();
    for _ in 0..8 {
        fixtures.push(build_fixture_with_counters(1, 1, 1, 2, Some(shared_counters.clone())).await);
    }
    let started = Instant::now();
    stream::iter(&fixtures)
        .map(|fixture| async move {
            fixture
                .market_data
                .prepare_valuation_market_data(&fixture.db, &fixture.assets, START, START)
                .await
                .expect("delayed candidate preparation");
        })
        .buffer_unordered(limit)
        .collect::<Vec<_>>()
        .await;
    let calls = shared_counters.source_calls.load(Ordering::Relaxed);
    let peak = shared_counters.peak.load(Ordering::Relaxed);
    let active = shared_counters.active.load(Ordering::Relaxed);
    let intervals = shared_counters.requested_intervals.lock().unwrap();
    assert_eq!(
        calls, 8,
        "each independent workload must make one source call"
    );
    assert!(peak <= limit, "peak source activity must respect the limit");
    assert_eq!(peak, limit, "all configured source slots must be exercised");
    assert_eq!(active, 0, "all source calls must finish");
    assert_eq!(
        intervals.len(),
        calls,
        "every source call records an interval"
    );
    let start = NaiveDate::parse_from_str(START, "%Y-%m-%d").unwrap();
    assert!(intervals.iter().all(|interval| interval == &(start, start)));
    (started.elapsed(), calls, peak)
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
                    &representative.end,
                )
                .await
                .unwrap()
        });
    });
    group.bench_function("nav_readiness_warm_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            nav::ensure_portfolio_history(&representative.db, &representative.market_data)
                .await
                .unwrap()
        });
    });
    group.bench_function("portfolio_retrieval_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            portfolio::get_portfolio(&representative.db, &representative.market_data)
                .await
                .unwrap()
        });
    });
    group.bench_function("correlation_matrix_representative", |b| {
        b.to_async(&runtime).iter(|| async {
            analytics::compute_correlation_data(
                &representative.db,
                START,
                &representative.end,
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
                &representative.end,
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
        let calls_before_warm = fixture.counters.source_calls.load(Ordering::Relaxed);
        runtime
            .block_on(fixture.market_data.prepare_valuation_market_data(
                &fixture.db,
                &fixture.assets,
                START,
                END,
            ))
            .unwrap();
        println!(
            "warm preparation source_calls={}",
            fixture.counters.source_calls.load(Ordering::Relaxed) - calls_before_warm
        );
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
                    .expect("cold portfolio retrieval");
                elapsed += started.elapsed();
            }
            elapsed
        });
    });
    runtime
        .block_on(portfolio::get_portfolio(&fixture.db, &fixture.market_data))
        .expect("warm setup");
    group.bench_function("portfolio_retrieval_warm", |b| {
        b.to_async(&runtime).iter(|| async {
            portfolio::get_portfolio(&fixture.db, &fixture.market_data)
                .await
                .unwrap()
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
    group.bench_function("startup_and_migration_transactional", |b| {
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
                    rstock::db::migrate(&db).await.unwrap();
                });
                elapsed += started.elapsed();
            }
            elapsed
        });
    });
    let binary = release_binary();
    assert!(
        binary.is_file(),
        "build the release executable before running startup benchmarks"
    );
    let logging_binary = release_logging_binary();
    assert!(
        logging_binary.is_file(),
        "build the release logging executable before running startup benchmarks"
    );
    group.bench_function("startup_executable_only", |b| {
        let home = tempfile::tempdir().unwrap();
        b.iter_custom(|iterations| {
            let started = Instant::now();
            for _ in 0..iterations {
                run_rstock(&binary, home.path(), &["--help"]);
            }
            started.elapsed()
        });
    });
    group.bench_function("startup_logging_cold", |b| {
        b.iter_custom(|iterations| {
            let directories: Vec<_> = (0..iterations)
                .map(|_| tempfile::tempdir().unwrap())
                .collect();
            let started = Instant::now();
            for directory in &directories {
                run_logging_init(&logging_binary, directory.path());
            }
            started.elapsed()
        });
    });
    group.bench_function("startup_logging_warm", |b| {
        let directory = tempfile::tempdir().unwrap();
        run_logging_init(&logging_binary, directory.path());
        b.iter_custom(|iterations| {
            let started = Instant::now();
            for _ in 0..iterations {
                run_logging_init(&logging_binary, directory.path());
            }
            started.elapsed()
        });
    });
    group.bench_function("startup_database_connection_cold", |b| {
        b.iter_custom(|iterations| {
            let directories: Vec<_> = (0..iterations)
                .map(|_| tempfile::tempdir().unwrap())
                .collect();
            let started = Instant::now();
            for directory in &directories {
                runtime.block_on(async {
                    let path = directory.path().join("connection.db");
                    let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
                        .await
                        .unwrap();
                    std::hint::black_box(db);
                });
            }
            started.elapsed()
        });
    });
    group.bench_function("startup_database_connection_warm", |b| {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("connection.db");
        runtime.block_on(async {
            let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .unwrap();
            std::hint::black_box(db);
        });
        b.to_async(&runtime).iter(|| async {
            let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .unwrap();
            std::hint::black_box(db);
        });
    });
    group.bench_function("startup_automatic_migration_cold", |b| {
        b.iter_custom(|iterations| {
            let databases = runtime.block_on(async {
                let mut databases = Vec::new();
                for _ in 0..iterations {
                    let directory = tempfile::tempdir().unwrap();
                    let path = directory.path().join("migration.db");
                    let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
                        .await
                        .unwrap();
                    databases.push((directory, db));
                }
                databases
            });
            let started = Instant::now();
            for (_, db) in &databases {
                runtime.block_on(rstock::db::migrate(db)).unwrap();
            }
            started.elapsed()
        });
    });
    group.bench_function("startup_automatic_migration_warm", |b| {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("migration.db");
        let db = runtime.block_on(async {
            let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .unwrap();
            rstock::db::migrate(&db).await.unwrap();
            db
        });
        b.to_async(&runtime)
            .iter(|| async { rstock::db::migrate(&db).await.unwrap() });
    });
    group.bench_function("startup_automatic_migration_warm_unbatched", |b| {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("migration.db");
        let db = runtime.block_on(async {
            let db = Database::connect(format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .unwrap();
            Migrator::up(&db, None).await.unwrap();
            db
        });
        b.to_async(&runtime)
            .iter(|| async { Migrator::up(&db, None).await.unwrap() });
    });
    group.bench_function("startup_transaction_list_cold", |b| {
        b.iter_custom(|iterations| {
            let homes: Vec<_> = (0..iterations)
                .map(|_| tempfile::tempdir().unwrap())
                .collect();
            let started = Instant::now();
            for home in &homes {
                run_rstock(&binary, home.path(), &["transaction", "list"]);
            }
            started.elapsed()
        });
    });
    group.bench_function("startup_transaction_list_warm", |b| {
        let home = tempfile::tempdir().unwrap();
        run_rstock(&binary, home.path(), &["transaction", "list"]);
        b.iter_custom(|iterations| {
            let started = Instant::now();
            for _ in 0..iterations {
                run_rstock(&binary, home.path(), &["transaction", "list"]);
            }
            started.elapsed()
        });
    });
    for limit in [1_usize, 2, 4, 8] {
        group.bench_function(format!("delayed_source_limit_{limit}"), |b| {
            b.iter_custom(|iterations| {
                let mut elapsed = std::time::Duration::ZERO;
                for _ in 0..iterations {
                    let (sample, calls, peak) = runtime.block_on(run_delayed_candidate(limit));
                    elapsed += sample;
                    println!(
                        "delayed limit={limit} calls={calls} peak={peak} elapsed_ns={}",
                        sample.as_nanos()
                    );
                }
                elapsed
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
