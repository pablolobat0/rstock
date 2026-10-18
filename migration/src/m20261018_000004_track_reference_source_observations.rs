use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Existing cached dates may include prior forward-fill output; without
        // provenance, conservatively leave them ineligible as genuine marks.
        manager
            .alter_table(
                Table::alter()
                    .table(BenchmarkReferencePrices::Table)
                    .add_column(
                        ColumnDef::new(BenchmarkReferencePrices::IsSourceObservation)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(BenchmarkReferencePrices::Table)
                    .drop_column(BenchmarkReferencePrices::IsSourceObservation)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum BenchmarkReferencePrices {
    Table,
    IsSourceObservation,
}
