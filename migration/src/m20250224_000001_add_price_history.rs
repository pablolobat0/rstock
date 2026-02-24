use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(DailyAssetPrices::Table)
                    .if_not_exists()
                    .col(pk_auto(DailyAssetPrices::Id))
                    .col(integer(DailyAssetPrices::AssetId))
                    .col(string(DailyAssetPrices::Date))
                    .col(double(DailyAssetPrices::ClosingPrice))
                    .col(boolean(DailyAssetPrices::IsApiFailure).default(false))
                    .foreign_key(
                        ForeignKey::create()
                            .from(DailyAssetPrices::Table, DailyAssetPrices::AssetId)
                            .to(Assets::Table, Assets::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_daily_asset_prices_asset_date")
                    .table(DailyAssetPrices::Table)
                    .col(DailyAssetPrices::AssetId)
                    .col(DailyAssetPrices::Date)
                    .unique()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(PortfolioHistory::Table)
                    .if_not_exists()
                    .col(string(PortfolioHistory::Date).primary_key())
                    .col(double(PortfolioHistory::CashBalance))
                    .col(double(PortfolioHistory::AssetValue))
                    .col(double(PortfolioHistory::TotalValue))
                    .col(double(PortfolioHistory::OutstandingShares))
                    .col(double(PortfolioHistory::Nav))
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PortfolioHistory::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(DailyAssetPrices::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum DailyAssetPrices {
    Table,
    Id,
    AssetId,
    Date,
    ClosingPrice,
    IsApiFailure,
}

#[derive(DeriveIden)]
enum PortfolioHistory {
    Table,
    Date,
    CashBalance,
    AssetValue,
    TotalValue,
    OutstandingShares,
    Nav,
}

#[derive(DeriveIden)]
enum Assets {
    Table,
    Id,
}
