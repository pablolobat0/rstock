use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use sea_orm::DatabaseConnection;

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

pub(crate) fn instrument_nav_execution_connection(db: &DatabaseConnection) -> DatabaseConnection {
    let mut instrumented = db.clone();
    instrumented.set_metric_callback(|info| {
        if is_read_statement(&info.statement.sql) {
            record_nav_sql_read();
        }
    });
    instrumented
}

fn record_nav_sql_read() {
    let _ = NAV_EXECUTION_READS.try_with(|reads| reads.fetch_add(1, Ordering::Relaxed));
}

fn is_read_statement(sql: &str) -> bool {
    let keyword = sql
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    matches!(keyword.as_str(), "SELECT" | "WITH" | "PRAGMA" | "EXPLAIN")
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

    use super::{instrument_nav_execution_connection, with_nav_execution_probe};

    #[tokio::test]
    async fn execution_probe_captures_a_real_raw_sql_read() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "CREATE TABLE probe (value INTEGER)",
        ))
        .await
        .unwrap();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO probe (value) VALUES (1)",
        ))
        .await
        .unwrap();
        let instrumented = instrument_nav_execution_connection(&db);

        let (result, reads) = with_nav_execution_probe(async {
            instrumented
                .query_one(Statement::from_string(
                    DbBackend::Sqlite,
                    "SELECT value FROM probe",
                ))
                .await
        })
        .await;

        assert!(result.unwrap().is_some());
        assert_eq!(reads, 1);
    }
}
