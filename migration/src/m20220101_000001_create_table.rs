use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Assets::Table)
                    .if_not_exists()
                    .col(pk_auto(Assets::Id))
                    .col(string_uniq(Assets::Ticker))
                    .col(string_null(Assets::Isin))
                    .col(string(Assets::Name))
                    .col(string(Assets::AssetType))
                    .col(string(Assets::Currency).default("EUR"))
                    .col(string(Assets::CreatedAt))
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(Transactions::Table)
                    .if_not_exists()
                    .col(pk_auto(Transactions::Id))
                    .col(integer(Transactions::AssetId))
                    .col(string(Transactions::TxType))
                    .col(string(Transactions::Date))
                    .col(double(Transactions::Quantity))
                    .col(big_integer(Transactions::PriceCents))
                    .col(big_integer(Transactions::FeesCents).default(0))
                    .col(string_null(Transactions::Notes))
                    .col(string(Transactions::CreatedAt))
                    .foreign_key(
                        ForeignKey::create()
                            .from(Transactions::Table, Transactions::AssetId)
                            .to(Assets::Table, Assets::Id),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Transactions::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Assets::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Assets {
    Table,
    Id,
    Ticker,
    Isin,
    Name,
    AssetType,
    Currency,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Transactions {
    Table,
    Id,
    AssetId,
    TxType,
    Date,
    Quantity,
    PriceCents,
    FeesCents,
    Notes,
    CreatedAt,
}
