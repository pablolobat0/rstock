use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

pub mod asset_repo;
pub mod daily_price_repo;
pub mod exchange_rate_repo;
pub mod fund_holdings_snapshot_repo;
pub mod portfolio_asset_history_repo;
pub mod portfolio_history_repo;
pub mod transaction_repo;

tokio::task_local! {
    static NAV_EXECUTION_READS: Arc<AtomicUsize>;
}

pub(crate) async fn with_nav_execution_probe<F, T>(future: F) -> (T, usize)
where
    F: Future<Output = T>,
{
    let reads = Arc::new(AtomicUsize::new(0));
    let result = NAV_EXECUTION_READS.scope(reads.clone(), future).await;
    (result, reads.load(Ordering::Acquire))
}

pub(crate) fn record_nav_database_read() {
    let _ = NAV_EXECUTION_READS.try_with(|reads| reads.fetch_add(1, Ordering::Relaxed));
}
