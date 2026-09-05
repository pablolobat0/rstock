use sea_orm_migration::prelude::*;

/// Replaces the historical overloaded transaction columns with a semantic
/// shape.  SQLite cannot alter CHECK constraints in place, so this migration
/// deliberately rebuilds the table in both directions.  The copy statements
/// are explicit to make the ID and `(date, id)` chronology contract visible.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP INDEX IF EXISTS idx_transactions_date_id")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_transactions_asset_date_id")
            .await?;
        db.execute_unprepared("ALTER TABLE transactions RENAME TO transactions_legacy")
            .await?;
        db.execute_unprepared(SEMANTIC_TABLE_SQL).await?;
        db.execute_unprepared(
            "INSERT INTO transactions (
                id, asset_id, tx_type, date, units, unit_price_cents,
                dividend_amount_cents, dividend_deductions_cents, split_ratio,
                fees_cents, notes, created_at
             )
             SELECT id, asset_id, tx_type, date,
                CASE WHEN tx_type IN ('buy', 'sell') THEN quantity END,
                CASE WHEN tx_type IN ('buy', 'sell') THEN price_cents END,
                CASE WHEN tx_type = 'dividend' THEN price_cents END,
                CASE WHEN tx_type = 'dividend' THEN fees_cents END,
                CASE WHEN tx_type = 'split' THEN quantity END,
                CASE WHEN tx_type IN ('buy', 'sell') THEN fees_cents END,
                notes, created_at
             FROM transactions_legacy",
        )
        .await?;
        db.execute_unprepared("DROP TABLE transactions_legacy")
            .await?;
        create_indexes(db).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP INDEX IF EXISTS idx_transactions_date_id")
            .await?;
        db.execute_unprepared("DROP INDEX IF EXISTS idx_transactions_asset_date_id")
            .await?;
        db.execute_unprepared("ALTER TABLE transactions RENAME TO transactions_semantic")
            .await?;
        db.execute_unprepared(LEGACY_TABLE_SQL).await?;
        db.execute_unprepared(
            "INSERT INTO transactions (
                id, asset_id, tx_type, date, quantity, price_cents,
                fees_cents, notes, created_at
             )
             SELECT id, asset_id, tx_type, date,
                CASE
                    WHEN tx_type IN ('buy', 'sell') THEN units
                    WHEN tx_type = 'dividend' THEN 1.0
                    WHEN tx_type = 'split' THEN split_ratio
                END,
                CASE
                    WHEN tx_type IN ('buy', 'sell') THEN unit_price_cents
                    WHEN tx_type = 'dividend' THEN dividend_amount_cents
                    WHEN tx_type = 'split' THEN 0
                END,
                CASE
                    WHEN tx_type IN ('buy', 'sell') THEN fees_cents
                    WHEN tx_type = 'dividend' THEN dividend_deductions_cents
                    WHEN tx_type = 'split' THEN 0
                END,
                notes, created_at
             FROM transactions_semantic",
        )
        .await?;
        db.execute_unprepared("DROP TABLE transactions_semantic")
            .await?;
        create_indexes(db).await
    }
}

const SEMANTIC_TABLE_SQL: &str = r#"
CREATE TABLE transactions (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    asset_id INTEGER NOT NULL,
    tx_type TEXT NOT NULL,
    date TEXT NOT NULL,
    units REAL,
    unit_price_cents INTEGER,
    dividend_amount_cents INTEGER,
    dividend_deductions_cents INTEGER,
    split_ratio REAL,
    fees_cents INTEGER,
    notes TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (asset_id) REFERENCES assets(id),
    CHECK (tx_type IN ('buy', 'sell', 'dividend', 'split')),
    CHECK (
        (tx_type IN ('buy', 'sell')
            AND units IS NOT NULL AND units > 0
            AND unit_price_cents IS NOT NULL AND unit_price_cents > 0
            AND fees_cents IS NOT NULL AND fees_cents >= 0
            AND dividend_amount_cents IS NULL
            AND dividend_deductions_cents IS NULL
            AND split_ratio IS NULL)
        OR
        (tx_type = 'dividend'
            AND units IS NULL
            AND unit_price_cents IS NULL
            AND dividend_amount_cents IS NOT NULL AND dividend_amount_cents > 0
            AND dividend_deductions_cents IS NOT NULL
            AND dividend_deductions_cents >= 0
            AND dividend_deductions_cents <= dividend_amount_cents
            AND split_ratio IS NULL AND fees_cents IS NULL)
        OR
        (tx_type = 'split'
            AND units IS NULL AND unit_price_cents IS NULL
            AND dividend_amount_cents IS NULL
            AND dividend_deductions_cents IS NULL
            AND split_ratio IS NOT NULL AND split_ratio > 0
            AND fees_cents IS NULL)
    )
)
"#;

const LEGACY_TABLE_SQL: &str = r#"
CREATE TABLE transactions (
    id INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    asset_id INTEGER NOT NULL,
    tx_type TEXT NOT NULL,
    date TEXT NOT NULL,
    quantity REAL NOT NULL,
    price_cents INTEGER NOT NULL,
    fees_cents INTEGER NOT NULL DEFAULT 0,
    notes TEXT,
    created_at TEXT NOT NULL,
    FOREIGN KEY (asset_id) REFERENCES assets(id)
)
"#;

async fn create_indexes(db: &impl ConnectionTrait) -> Result<(), DbErr> {
    db.execute_unprepared(
        "CREATE INDEX idx_transactions_date_id ON transactions (date, id)",
    )
    .await?;
    db.execute_unprepared(
        "CREATE INDEX idx_transactions_asset_date_id ON transactions (asset_id, date, id)",
    )
    .await?;
    Ok(())
}
