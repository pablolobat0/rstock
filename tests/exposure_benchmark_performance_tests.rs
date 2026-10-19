//! Exposure-matched benchmark performance evidence.
//!
//! The measurements separate the *incremental* cost of the exposure-matched
//! comparison on an already ready NAV history from the cost of the NAV
//! rebuild itself. The comparison reuses the ready history: it performs no
//! ledger replay, no NAV rebuild, and no snapshot write inside a measured
//! window.
//!
//! The file holds exactly one test on purpose. The counting allocator below
//! observes every allocation of the test process, so a second test running
//! in parallel would pollute the peak live bytes. The measured window also
//! excludes the fixture construction, which is completed before the window
//! opens.
//!
//! The recorded numbers are work evidence, not a performance budget. The user
//! never set a millisecond, memory, read-count, or source-attempt target, so
//! this file asserts structural properties and records measurements; it does
//! not invent a pass or fail threshold, and it does not substitute monthly
//! exposure weights for daily ones.

#![allow(clippy::float_cmp)]

pub mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use sea_orm::{DatabaseConnection, EntityTrait, Set};

use common::{market_data_at, setup_test_db, MockMarketDataSources};
use rstock::db::entities::{
    benchmark_assignment, benchmark_reference, portfolio_asset_history, portfolio_history,
};
use rstock::models::PortfolioSnapshot;
use rstock::services::exposure_benchmark::compute_exposure_matched_comparison;

// --- Allocation work proxy ---

struct CountingAllocator;

static WINDOW_OPEN: AtomicBool = AtomicBool::new(false);
static WINDOW_ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static DEALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

/// The byte-aware allocator profile of one measured window.
#[derive(Clone, Copy, Debug, Default)]
struct AllocationWindow {
    allocations: usize,
    allocated_bytes: usize,
    deallocated_bytes: usize,
    /// The running balance of window allocations and window drops.
    final_live_bytes: usize,
    /// The maximum live balance observed during the window.
    peak_live_bytes: usize,
}

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc(layout);
        if !pointer.is_null() && WINDOW_OPEN.load(Ordering::Relaxed) {
            record_window_alloc(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = System.alloc_zeroed(layout);
        if !pointer.is_null() && WINDOW_OPEN.load(Ordering::Relaxed) {
            record_window_alloc(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if WINDOW_OPEN.load(Ordering::Relaxed) {
            record_window_dealloc(layout.size());
        }
        System.dealloc(pointer, layout);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let pointer = System.realloc(pointer, layout, new_size);
        if !pointer.is_null() && WINDOW_OPEN.load(Ordering::Relaxed) {
            record_window_realloc(layout.size(), new_size);
        }
        pointer
    }
}

fn record_window_alloc(size: usize) {
    let live = LIVE_BYTES.fetch_add(size, Ordering::Relaxed) + size;
    PEAK_LIVE_BYTES.fetch_max(live, Ordering::Relaxed);
    WINDOW_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
}

fn record_window_realloc(old_size: usize, new_size: usize) {
    let live = if new_size > old_size {
        LIVE_BYTES.fetch_add(new_size - old_size, Ordering::Relaxed) + (new_size - old_size)
    } else {
        shrink_live_bytes_saturating(old_size - new_size)
    };
    PEAK_LIVE_BYTES.fetch_max(live, Ordering::Relaxed);
    WINDOW_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(new_size, Ordering::Relaxed);
    DEALLOCATED_BYTES.fetch_add(old_size, Ordering::Relaxed);
}

fn record_window_dealloc(size: usize) {
    let _live = shrink_live_bytes_saturating(size);
    DEALLOCATED_BYTES.fetch_add(size, Ordering::Relaxed);
}

fn shrink_live_bytes_saturating(size: usize) -> usize {
    let mut current = LIVE_BYTES.load(Ordering::Relaxed);
    loop {
        let next = current.saturating_sub(size);
        match LIVE_BYTES.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => return next,
            Err(observed) => current = observed,
        }
    }
}

fn open_allocation_window() {
    LIVE_BYTES.store(0, Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(0, Ordering::Relaxed);
    WINDOW_ALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    DEALLOCATED_BYTES.store(0, Ordering::Relaxed);
    WINDOW_OPEN.store(true, Ordering::Relaxed);
}

fn close_allocation_window() -> AllocationWindow {
    WINDOW_OPEN.store(false, Ordering::Relaxed);
    AllocationWindow {
        allocations: WINDOW_ALLOCATIONS.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        final_live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        peak_live_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
    }
}

// --- Database read work proxy ---

/// Counts the SQL read statements one measured scope issues.
fn instrument_reads(db: &DatabaseConnection) -> (DatabaseConnection, Arc<AtomicUsize>) {
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reads);
    let mut instrumented = db.clone();
    instrumented.set_metric_callback(move |info| {
        let sql = info.statement.sql.trim_start().to_ascii_uppercase();
        if sql.starts_with("SELECT") || sql.starts_with("WITH") || sql.starts_with("PRAGMA") {
            counter.fetch_add(1, Ordering::Relaxed);
        }
    });
    (instrumented, reads)
}

// --- Fixtures ---

/// One measured comparison run.
#[derive(Clone, Copy, Debug)]
struct Measurement {
    label: &'static str,
    dates: usize,
    assets: usize,
    references: usize,
    /// The elapsed wall time of the whole comparison call.
    elapsed: Duration,
    /// The elapsed wall time of one separate bounded exposure read.
    read_elapsed: Duration,
    /// The persisted exposure rows the bounded read returns.
    read_rows: usize,
    database_reads: usize,
    /// Every source attempt, including the benchmark predecessor lookups.
    source_attempts: usize,
    allocations: AllocationWindow,
}

/// A ready NAV history with one verified reference for every holding.
struct Fixture {
    db: DatabaseConnection,
    sources: MockMarketDataSources,
    snapshots: Vec<PortfolioSnapshot>,
    reference_count: usize,
    asset_count: usize,
}

/// Builds a ready Complete NAV history of `assets` holdings over `days`
/// calendar days, with one shared verified reference for every holding.
///
/// The history is persisted directly, exactly as the NAV rebuild would have
/// persisted it, so the measured window contains no NAV rebuild work.
#[allow(clippy::too_many_lines)]
async fn build_ready_fixture(assets: usize, days: usize) -> Fixture {
    let db = setup_test_db().await;
    let start = NaiveDate::from_ymd_opt(2020, 1, 1).expect("fixed date");

    let mut asset_ids = Vec::with_capacity(assets);
    for index in 0..assets {
        let ticker = format!("XPERF{index:03}");
        let asset_id =
            common::insert_asset(&db, &ticker, "Performance fixture", "stock", "EUR").await;
        asset_ids.push(asset_id);
    }

    benchmark_reference::Entity::insert(benchmark_reference::ActiveModel {
        id: Set("perf-reference".to_owned()),
        display_name: Set("Performance Reference".to_owned()),
        instrument_ticker: Set("PERFREF".to_owned()),
        source: Set("test_source".to_owned()),
        source_identifier: Set("PERFREF".to_owned()),
        currency: Set("EUR".to_owned()),
        adjustment_provenance: Set("explicit fake total-return observations".to_owned()),
        adjustment_qualification: Set("dividends_reinvested_once".to_owned()),
        return_basis_qualification: Set("total_return".to_owned()),
        role: Set("Investment universe".to_owned()),
        universe: Set("test".to_owned()),
        hedge_policy: Set("Unhedged".to_owned()),
        source_provenance: Set("deterministic test source".to_owned()),
        history_boundary: Set("deterministic fixture".to_owned()),
        verification_state: Set("verified".to_owned()),
        ..Default::default()
    })
    .exec(&db)
    .await
    .expect("reference definition should persist");
    for (index, _asset_id) in asset_ids.iter().enumerate() {
        let ticker = format!("XPERF{index:03}");
        benchmark_assignment::Entity::insert(benchmark_assignment::ActiveModel {
            subject_type: Set("tracked".to_owned()),
            subject_id: Set(ticker),
            reference_id: Set("perf-reference".to_owned()),
            is_override: Set(false),
            assigned_at: Set("2020-01-01T00:00:00Z".to_owned()),
        })
        .exec(&db)
        .await
        .expect("assignment should persist");
    }

    let mut sources = MockMarketDataSources::new();
    let mut reference_prices = Vec::with_capacity(days + 1);
    let mut snapshots = Vec::with_capacity(days + 1);
    let mut asset_rows = Vec::with_capacity(days * assets);
    for day in 0..=days {
        let date = start + chrono::Duration::days(i64::try_from(day).expect("fixture day count"));
        let date_string = rstock::constants::format_date(date);
        // Whole-unit prices keep the persisted asset value and the persisted
        // per-asset market values exactly consistent.
        let price = 100.0 + day as f64;
        let day_asset_value = assets as f64 * price;
        reference_prices.push((date_string.clone(), 100.0 + day as f64 * 0.1));
        snapshots.push(PortfolioSnapshot {
            date: date_string.clone(),
            asset_value: day_asset_value,
            total_value: day_asset_value,
            outstanding_shares: day as f64 + 1.0,
            nav: 100.0,
        });
        for asset_id in &asset_ids {
            asset_rows.push(portfolio_asset_history::ActiveModel {
                date: Set(date_string.clone()),
                asset_id: Set(*asset_id),
                quantity: Set(1.0),
                closing_price: Set(price),
                market_value: Set(price),
                exchange_rate: Set(1.0),
                ..Default::default()
            });
        }
    }

    for chunk in snapshots.chunks(500) {
        portfolio_history::Entity::insert_many(chunk.iter().map(|snapshot| {
            portfolio_history::ActiveModel {
                date: Set(snapshot.date.clone()),
                asset_value: Set(snapshot.asset_value),
                total_value: Set(snapshot.total_value),
                outstanding_shares: Set(snapshot.outstanding_shares),
                nav: Set(snapshot.nav),
            }
        }))
        .exec(&db)
        .await
        .expect("NAV snapshots should persist");
    }
    for chunk in asset_rows.chunks(500) {
        portfolio_asset_history::Entity::insert_many(chunk.iter().cloned())
            .exec(&db)
            .await
            .expect("per-asset exposures should persist");
    }

    sources
        .historical_prices
        .insert("PERFREF".to_owned(), reference_prices);
    sources.series_qualifications.insert(
        ("test_source".to_owned(), "PERFREF".to_owned()),
        rstock::services::market_data::SeriesQualification {
            currency: "EUR".to_owned(),
            return_basis: "total_return".to_owned(),
            adjustment: "dividends_reinvested_once".to_owned(),
            hedge_policy: "Unhedged".to_owned(),
            cadence: "daily".to_owned(),
        },
    );

    Fixture {
        db,
        sources,
        snapshots,
        reference_count: 1,
        asset_count: assets,
    }
}

/// Measures one exposure-matched comparison over a ready NAV history.
async fn measure(label: &'static str, fixture: &Fixture) -> Measurement {
    let market_data = market_data_at(&fixture.sources, fixture_today(fixture));
    let (instrumented, reads) = instrument_reads(&fixture.db);
    let source_attempts_before = fixture.sources.historical_source_call_count();

    // The bounded exposure read is measured on its own, as a separate
    // measurement of that read. It is not subtracted from the comparison
    // timing, because the two calls do not share one instrumented scope.
    let read_started = Instant::now();
    let read_rows = rstock::db::repos::portfolio_asset_history_repo::find_between(
        &instrumented,
        &fixture.snapshots[0].date,
        &fixture.snapshots[fixture.snapshots.len() - 1].date,
    )
    .await
    .expect("the bounded exposure read should succeed")
    .len();
    let read_elapsed = read_started.elapsed();
    let reads_after_read = reads.load(Ordering::Relaxed);

    open_allocation_window();
    let started = Instant::now();
    let comparison =
        compute_exposure_matched_comparison(&instrumented, &market_data, &fixture.snapshots)
            .await
            .expect("the exposure comparison should be computable");
    let elapsed = started.elapsed();
    let allocations = close_allocation_window();

    assert!(
        matches!(
            comparison.availability,
            rstock::models::ComparisonAvailability::Available
        ),
        "the performance fixture must produce an available comparison"
    );
    assert!(
        reads.load(Ordering::Relaxed) > reads_after_read,
        "the comparison must read the persisted exposures itself"
    );
    Measurement {
        label,
        dates: fixture.snapshots.len(),
        assets: fixture.asset_count,
        references: fixture.reference_count,
        elapsed,
        read_elapsed,
        read_rows,
        database_reads: reads.load(Ordering::Relaxed) - reads_after_read,
        source_attempts: fixture.sources.historical_source_call_count() - source_attempts_before,
        allocations,
    }
}

fn fixture_today(fixture: &Fixture) -> NaiveDate {
    let last = fixture
        .snapshots
        .last()
        .expect("the fixture has snapshots")
        .date
        .clone();
    NaiveDate::parse_from_str(&last, rstock::constants::DATE_FORMAT).expect("valid fixture date")
        + chrono::Duration::days(1)
}

// --- The single measurement test ---

/// The recorded representative and stress work evidence, and the structural
/// properties that keep the comparison from scaling with the number of
/// closing dates.
#[tokio::test]
async fn representative_and_stress_work_evidence_is_recorded() {
    let short = build_ready_fixture(4, 40).await;
    let long = build_ready_fixture(4, 400).await;
    let representative = build_ready_fixture(10, 365).await;
    let stress = build_ready_fixture(50, 3650).await;

    let short_measurement = measure("short", &short).await;
    let long_measurement = measure("long", &long).await;
    let representative_measurement = measure("representative", &representative).await;
    let stress_measurement = measure("stress", &stress).await;

    for measurement in [
        short_measurement,
        long_measurement,
        representative_measurement,
        stress_measurement,
    ] {
        println!(
            "exposure_benchmark_work label={} dates={} assets={} references={} elapsed_ns={} read_elapsed_ns={} read_rows={} database_reads={} source_attempts={} allocations={} allocated_bytes={} deallocated_bytes={} peak_live_bytes={} final_live_bytes={}",
            measurement.label,
            measurement.dates,
            measurement.assets,
            measurement.references,
            measurement.elapsed.as_nanos(),
            measurement.read_elapsed.as_nanos(),
            measurement.read_rows,
            measurement.database_reads,
            measurement.source_attempts,
            measurement.allocations.allocations,
            measurement.allocations.allocated_bytes,
            measurement.allocations.deallocated_bytes,
            measurement.allocations.peak_live_bytes,
            measurement.allocations.final_live_bytes,
        );
    }

    // The exposure snapshot read is one bounded range query: the read
    // statement count does not scale with the number of closing dates.
    assert!(
        long_measurement.dates > short_measurement.dates * 5,
        "the long fixture must cover materially more dates"
    );
    assert_eq!(
        long_measurement.database_reads, short_measurement.database_reads,
        "the exposure snapshot read must be one bounded range, not one query per date"
    );
    // Every source attempt is counted, including the benchmark predecessor
    // lookups, and they do not scale with the number of dates either.
    assert_eq!(
        long_measurement.source_attempts, short_measurement.source_attempts,
        "one history request and one predecessor lookup per reference, not per date"
    );
    assert_eq!(
        long_measurement.source_attempts,
        long_measurement.references * 2,
        "every source attempt must be counted"
    );
    assert!(
        stress_measurement.source_attempts <= stress_measurement.references * 2,
        "the stress fixture must not issue one source request per date"
    );
    assert!(stress_measurement.elapsed > Duration::ZERO);
    assert!(stress_measurement.allocations.allocations > 0);
}
