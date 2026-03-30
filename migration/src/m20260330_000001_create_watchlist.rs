use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Watchlist::Table)
                    .if_not_exists()
                    .col(pk_auto(Watchlist::Id))
                    .col(string_uniq(Watchlist::Ticker))
                    .col(string(Watchlist::SectorEtfTicker))
                    .col(string(Watchlist::CreatedAt))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Watchlist::Table).to_owned())
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Watchlist {
    Table,
    Id,
    Ticker,
    SectorEtfTicker,
    CreatedAt,
}
