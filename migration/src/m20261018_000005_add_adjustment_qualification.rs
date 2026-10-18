use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(BenchmarkReferences::Table)
                    .add_column(
                        ColumnDef::new(BenchmarkReferences::AdjustmentQualification)
                            .string()
                            .not_null()
                            .default(""),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(BenchmarkReferences::Table)
                    .drop_column(BenchmarkReferences::AdjustmentQualification)
                    .to_owned(),
            )
            .await
    }
}

#[derive(DeriveIden)]
enum BenchmarkReferences {
    Table,
    AdjustmentQualification,
}
