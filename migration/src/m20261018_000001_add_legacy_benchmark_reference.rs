use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkReferences::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(BenchmarkReferences::Id)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkReferences::DisplayName)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkReferences::InstrumentTicker)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(BenchmarkReferences::Source).string().not_null())
                    .col(
                        ColumnDef::new(BenchmarkReferences::SourceIdentifier)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkReferences::Currency)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkReferences::AdjustmentProvenance)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkReferences::ReturnBasisQualification)
                            .string()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkReferencePrices::Table)
                    .if_not_exists()
                    .col(ColumnDef::new(BenchmarkReferencePrices::Id).integer().not_null().auto_increment().primary_key())
                    .col(ColumnDef::new(BenchmarkReferencePrices::ReferenceId).string().not_null())
                    .col(ColumnDef::new(BenchmarkReferencePrices::Date).string().not_null())
                    .col(ColumnDef::new(BenchmarkReferencePrices::ClosingPrice).double().not_null())
                    .col(ColumnDef::new(BenchmarkReferencePrices::IsApiFailure).boolean().not_null().default(false))
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_reference_prices_reference")
                            .from(BenchmarkReferencePrices::Table, BenchmarkReferencePrices::ReferenceId)
                            .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .index(
                        Index::create()
                            .name("idx_benchmark_reference_prices_reference_date")
                            .col(BenchmarkReferencePrices::ReferenceId)
                            .col(BenchmarkReferencePrices::Date)
                            .unique(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO benchmark_references (
                    id, display_name, instrument_ticker, source, source_identifier, currency,
                    adjustment_provenance, return_basis_qualification
                 ) VALUES (
                    'legacy-acwi-etf-proxy', 'iShares MSCI ACWI ETF (ACWI) proxy', 'ACWI',
                    'yahoo_finance', 'ACWI', 'USD',
                    'Yahoo Finance history via yfinance_rs with auto_adjust enabled by default',
                    'Adjusted ETF price basis; distribution treatment and total-return equivalence are unverified'
                 )",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "INSERT INTO benchmark_reference_prices (reference_id, date, closing_price, is_api_failure)
                 SELECT 'legacy-acwi-etf-proxy', prices.date, prices.closing_price, prices.is_api_failure
                 FROM daily_asset_prices AS prices
                 JOIN assets ON assets.id = prices.asset_id
                 WHERE assets.ticker = 'ACWI' AND assets.asset_type = 'stock' AND assets.currency = 'USD'",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(BenchmarkReferencePrices::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BenchmarkReferences::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum BenchmarkReferences {
    Table,
    Id,
    DisplayName,
    InstrumentTicker,
    Source,
    SourceIdentifier,
    Currency,
    AdjustmentProvenance,
    ReturnBasisQualification,
}

#[derive(DeriveIden)]
enum BenchmarkReferencePrices {
    Table,
    Id,
    ReferenceId,
    Date,
    ClosingPrice,
    IsApiFailure,
}
