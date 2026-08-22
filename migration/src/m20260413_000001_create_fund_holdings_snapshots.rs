use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(FundHoldingsSnapshots::Table)
                    .if_not_exists()
                    .col(pk_auto(FundHoldingsSnapshots::Id))
                    .col(string(FundHoldingsSnapshots::MsCode))
                    .col(string(FundHoldingsSnapshots::SnapshotDate))
                    .col(string(FundHoldingsSnapshots::Fingerprint))
                    .col(string(FundHoldingsSnapshots::HoldingsJson))
                    .col(
                        ColumnDef::new(FundHoldingsSnapshots::TotalHoldings)
                            .integer()
                            .null(),
                    )
                    .col(string(FundHoldingsSnapshots::CreatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_fhs_code_date")
                    .table(FundHoldingsSnapshots::Table)
                    .col(FundHoldingsSnapshots::MsCode)
                    .col(FundHoldingsSnapshots::SnapshotDate)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(FundHoldingsSnapshots::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum FundHoldingsSnapshots {
    Table,
    Id,
    MsCode,
    SnapshotDate,
    Fingerprint,
    HoldingsJson,
    TotalHoldings,
    CreatedAt,
}
