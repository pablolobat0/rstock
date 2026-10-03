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
