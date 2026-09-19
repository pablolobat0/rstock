use std::{
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::models::{AssetSnapshot, PortfolioSnapshot};

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

fn instrument_nav_execution_connection(db: &DatabaseConnection) -> DatabaseConnection {
    let mut instrumented = db.clone();
    instrumented.set_metric_callback(|info| {
        if is_read_statement(&info.statement.sql) {
            record_nav_sql_read();
        }
    });
    instrumented
}

/// Private persistence capability for generated NAV snapshots.
///
/// The connection is intentionally not exposed or used as a generic
/// `ConnectionTrait` by the NAV executor. This keeps execution limited to the
/// atomic snapshot write operation below.
pub(crate) struct NavSnapshotSink {
    db: DatabaseConnection,
}

impl NavSnapshotSink {
    pub(crate) fn new(db: &DatabaseConnection) -> Self {
        Self {
            db: instrument_nav_execution_connection(db),
        }
    }

    pub(crate) async fn persist(
        &self,
        portfolio_snapshots: &[PortfolioSnapshot],
        asset_snapshots: &[AssetSnapshot],
    ) -> anyhow::Result<()> {
        let transaction = self.db.begin().await?;
        portfolio_history_repo::upsert_many(&transaction, portfolio_snapshots).await?;
        portfolio_asset_history_repo::upsert_many(&transaction, asset_snapshots).await?;
        transaction.commit().await?;
        Ok(())
    }
}

fn record_nav_sql_read() {
    let _ = NAV_EXECUTION_READS.try_with(|reads| reads.fetch_add(1, Ordering::Relaxed));
}

fn is_read_statement(sql: &str) -> bool {
    let keyword = first_keyword(sql);
    match keyword.as_deref() {
        Some("SELECT" | "PRAGMA" | "EXPLAIN") => true,
        Some("WITH") => with_statement_is_read(sql),
        _ => false,
    }
}

fn first_keyword(sql: &str) -> Option<String> {
    let bytes = sql.as_bytes();
    let mut index = 0;
    loop {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index..index + 2) == Some(b"--") {
            index += 2;
            while bytes.get(index).is_some_and(|byte| *byte != b'\n') {
                index += 1;
            }
        } else if bytes.get(index..index + 2) == Some(b"/*") {
            let end = sql[index + 2..].find("*/")?;
            index += end + 4;
        } else {
            break;
        }
    }
    let start = index;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        index += 1;
    }
    (index > start).then(|| sql[start..index].to_ascii_uppercase())
}

fn with_statement_is_read(sql: &str) -> bool {
    let mut depth: usize = 0;
    let mut token = String::new();
    let mut quote: Option<u8> = None;
    let bytes = sql.as_bytes();
    let mut index = 0;
    while let Some(&character) = bytes.get(index) {
        if let Some(delimiter) = quote {
            if character == delimiter {
                if bytes.get(index + 1) == Some(&delimiter) {
                    index += 2;
                    continue;
                }
                quote = None;
            }
            index += 1;
            continue;
        }

        // Comments separate SQL tokens just like whitespace. Finish a pending
        // outer keyword before skipping a comment adjacent to that keyword.
        if matches!(bytes.get(index..index + 2), Some(b"--" | b"/*")) {
            if depth == 0 {
                match token.as_str() {
                    "SELECT" => return true,
                    "INSERT" | "UPDATE" | "DELETE" | "REPLACE" => return false,
                    _ => {}
                }
            }
            token.clear();
        }

        if bytes.get(index..index + 2) == Some(b"--") {
            index += 2;
            while bytes.get(index).is_some_and(|byte| *byte != b'\n') {
                index += 1;
            }
            continue;
        }
        if bytes.get(index..index + 2) == Some(b"/*") {
            let Some(end) = sql[index + 2..].find("*/") else {
                return false;
            };
            index += end + 4;
            continue;
        }
        if matches!(character, b'\'' | b'"' | b'`') {
            quote = Some(character);
            index += 1;
            continue;
        }
        if character == b'(' {
            depth += 1;
        } else if character == b')' {
            depth = depth.saturating_sub(1);
        }
        if character.is_ascii_alphabetic() || character == b'_' {
            token.push(character.to_ascii_uppercase() as char);
        } else if depth == 0 && !token.is_empty() {
            let is_read = token == "SELECT";
            let is_write = matches!(token.as_str(), "INSERT" | "UPDATE" | "DELETE" | "REPLACE");
            token.clear();
            if is_read {
                return true;
            }
            if is_write {
                return false;
            }
        } else {
            token.clear();
        }
        index += 1;
    }
    depth == 0 && token == "SELECT"
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement, TransactionTrait};

    use super::{instrument_nav_execution_connection, with_nav_execution_probe, NavSnapshotSink};

    #[tokio::test]
    async fn execution_probe_captures_real_sql_reads_and_ignores_writes() {
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

        let (_, commented_reads) = with_nav_execution_probe(async {
            instrumented
                .query_one(Statement::from_string(
                    DbBackend::Sqlite,
                    "-- a leading comment\nSELECT value FROM probe",
                ))
                .await
        })
        .await;
        assert_eq!(commented_reads, 1);

        let (_, inline_commented_reads) = with_nav_execution_probe(async {
            instrumented
                .query_one(Statement::from_string(
                    DbBackend::Sqlite,
                    "SELECT/* ignored comment */ 1",
                ))
                .await
        })
        .await;
        assert_eq!(inline_commented_reads, 1);

        let (_, unprepared_reads) = with_nav_execution_probe(async {
            instrumented
                .execute_unprepared("SELECT value FROM probe")
                .await
        })
        .await;
        assert_eq!(unprepared_reads, 0);

        let (_, transaction_reads) = with_nav_execution_probe(async {
            let sink = NavSnapshotSink::new(&db);
            sink.db
                .begin()
                .await
                .unwrap()
                .query_one(Statement::from_string(
                    DbBackend::Sqlite,
                    "SELECT value FROM probe",
                ))
                .await
                .map(|_| ())
        })
        .await;
        assert_eq!(transaction_reads, 1);

        let (_, commented_cte_reads) = with_nav_execution_probe(async {
            instrumented
                .query_one(Statement::from_string(
                    DbBackend::Sqlite,
                    "WITH rows AS (SELECT 1) /* UPDATE */ SELECT * FROM rows",
                ))
                .await
        })
        .await;
        assert_eq!(commented_cte_reads, 1);

        let (_, with_write_reads) = with_nav_execution_probe(async {
            instrumented
                .execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "WITH rows AS (SELECT 2) INSERT INTO probe (value) SELECT * FROM rows",
                ))
                .await
        })
        .await;
        assert_eq!(with_write_reads, 0);

        let (_, commented_cte_write_reads) = with_nav_execution_probe(async {
            instrumented
                .execute(Statement::from_string(
                    DbBackend::Sqlite,
                    "WITH rows AS (SELECT 2) /* SELECT */ INSERT INTO probe (value) SELECT * FROM rows",
                ))
                .await
        })
        .await;
        assert_eq!(commented_cte_write_reads, 0);
    }

    #[tokio::test]
    async fn execution_probe_treats_adjacent_cte_comments_as_token_separators() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "CREATE TABLE probe (value INTEGER)",
        ))
        .await
        .unwrap();
        let instrumented = instrument_nav_execution_connection(&db);

        for sql in [
            "WITH rows AS (SELECT 1 AS value) SELECT/* UPDATE */value FROM rows",
            "WITH rows AS (SELECT 1 AS value) SELECT-- UPDATE\nvalue FROM rows",
        ] {
            let (result, reads) = with_nav_execution_probe(async {
                instrumented
                    .query_one(Statement::from_string(DbBackend::Sqlite, sql))
                    .await
            })
            .await;
            assert!(result.unwrap().is_some(), "{sql}");
            assert_eq!(reads, 1, "{sql}");
        }

        for sql in [
            "WITH rows AS (SELECT 2) INSERT/* SELECT */INTO probe (value) SELECT * FROM rows",
            "WITH rows AS (SELECT 2) INSERT-- SELECT\nINTO probe (value) SELECT * FROM rows",
        ] {
            let (result, reads) = with_nav_execution_probe(async {
                instrumented
                    .execute(Statement::from_string(DbBackend::Sqlite, sql))
                    .await
            })
            .await;
            assert_eq!(result.unwrap().rows_affected(), 1, "{sql}");
            assert_eq!(reads, 0, "{sql}");
        }
    }
}
