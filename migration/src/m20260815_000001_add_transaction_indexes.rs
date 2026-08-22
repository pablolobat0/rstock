use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_index(
                Index::create()
                    .name("idx_transactions_date_id")
                    .table(Transactions::Table)
                    .col(Transactions::Date)
                    .col(Transactions::Id)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_transactions_asset_date_id")
                    .table(Transactions::Table)
                    .col(Transactions::AssetId)
                    .col(Transactions::Date)
                    .col(Transactions::Id)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_index(
                Index::drop()
                    .name("idx_transactions_asset_date_id")
                    .to_owned(),
            )
            .await?;
        manager
            .drop_index(Index::drop().name("idx_transactions_date_id").to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Transactions {
    Table,
    Id,
    AssetId,
    Date,
}
