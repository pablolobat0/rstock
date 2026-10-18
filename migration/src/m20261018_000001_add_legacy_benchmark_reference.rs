use sea_orm_migration::prelude::sea_orm::Statement;
use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(
            "CREATE TABLE benchmark_references (
                id TEXT NOT NULL PRIMARY KEY,
                instrument_ticker TEXT NOT NULL,
                display_name TEXT NOT NULL,
                source TEXT NOT NULL,
                adjustment_provenance TEXT NOT NULL,
                return_basis_qualification TEXT NOT NULL
            )",
        )
        .await?;
        db.execute(Statement::from_string(
            manager.get_database_backend(),
            "INSERT INTO benchmark_references (
                id, instrument_ticker, display_name, source,
                adjustment_provenance, return_basis_qualification
             ) VALUES (
                'legacy-acwi-etf-proxy', 'ACWI', 'iShares MSCI ACWI ETF (ACWI) proxy',
                'Yahoo Finance via yfinance_rs',
                'Yahoo Finance history requested through yfinance_rs; auto_adjust is enabled by default',
                'Adjusted ETF price basis; distribution treatment and total-return equivalence are unverified'
             )".to_owned(),
        )).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE benchmark_references")
            .await?;
        Ok(())
    }
}
