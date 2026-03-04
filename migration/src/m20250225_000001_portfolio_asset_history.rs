use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(PortfolioAssetHistory::Table)
                    .if_not_exists()
                    .col(pk_auto(PortfolioAssetHistory::Id))
                    .col(string(PortfolioAssetHistory::Date))
                    .col(integer(PortfolioAssetHistory::AssetId))
                    .col(double(PortfolioAssetHistory::Quantity))
                    .col(double(PortfolioAssetHistory::ClosingPrice))
                    .col(double(PortfolioAssetHistory::MarketValue))
                    .foreign_key(
                        ForeignKey::create()
                            .from(PortfolioAssetHistory::Table, PortfolioAssetHistory::AssetId)
                            .to(Assets::Table, Assets::Id),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_portfolio_asset_history_date_asset")
                    .table(PortfolioAssetHistory::Table)
                    .col(PortfolioAssetHistory::Date)
                    .col(PortfolioAssetHistory::AssetId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PortfolioAssetHistory::Table).to_owned())
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum PortfolioAssetHistory {
    Table,
    Id,
    Date,
    AssetId,
    Quantity,
    ClosingPrice,
    MarketValue,
}

#[derive(DeriveIden)]
enum Assets {
    Table,
    Id,
}
