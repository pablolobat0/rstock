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
    let sql = strip_leading_comments(sql);
    let keyword = sql
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    match keyword.as_str() {
        "SELECT" | "PRAGMA" | "EXPLAIN" => true,
        "WITH" => with_statement_is_read(sql),
        _ => false,
    }
}

fn strip_leading_comments(mut sql: &str) -> &str {
    loop {
        sql = sql.trim_start();
        if let Some(comment) = sql.strip_prefix("--") {
            sql = comment
                .find('\n')
                .map_or("", |newline| &comment[newline + 1..]);
        } else if let Some(comment) = sql.strip_prefix("/*") {
            sql = comment.find("*/").map_or("", |end| &comment[end + 2..]);
        } else {
            return sql;
        }
    }
}

fn with_statement_is_read(sql: &str) -> bool {
    let mut depth: usize = 0;
    let mut token = String::new();
    let mut quote = None;
    for character in sql.chars() {
        if let Some(delimiter) = quote {
            if character == delimiter {
                quote = None;
            }
            continue;
        }
        if matches!(character, '\'' | '"' | '`') {
            quote = Some(character);
            continue;
        }
        if character == '(' {
            depth += 1;
        } else if character == ')' {
            depth = depth.saturating_sub(1);
        }
        if character.is_ascii_alphabetic() || character == '_' {
            token.push(character.to_ascii_uppercase());
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
    }
    depth == 0 && token == "SELECT"
}

#[cfg(test)]
mod tests {
    use sea_orm::{ConnectionTrait, Database, DbBackend, Statement, TransactionTrait};

    use super::{instrument_nav_execution_connection, with_nav_execution_probe};

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

        let (_, unprepared_reads) = with_nav_execution_probe(async {
            instrumented
                .execute_unprepared("SELECT value FROM probe")
                .await
        })
        .await;
        assert_eq!(unprepared_reads, 0);

        let (_, transaction_reads) = with_nav_execution_probe(async {
            instrumented
                .transaction(|transaction| {
                    Box::pin(async move {
                        transaction
                            .query_one(Statement::from_string(
                                DbBackend::Sqlite,
                                "SELECT value FROM probe",
                            ))
                            .await
                            .map(|_| ())
                    })
                })
                .await
                .unwrap();
            Ok::<_, sea_orm::DbErr>(())
        })
        .await;
        assert_eq!(transaction_reads, 1);

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
    }
}
